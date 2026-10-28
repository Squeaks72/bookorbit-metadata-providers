use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::mapping::{
    clean_query, map_product, product_links, rank, slug_matches, wanted_part, BookMetadata,
};
use crate::site::{Client, UpstreamError};
use crate::{Config, VERSION};

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 20;
/// Product pages fetched for one search, however many the series page links.
const MAX_FETCHES: usize = 8;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ID_LENGTH: usize = 120;

#[derive(Clone)]
struct AppState {
    cfg: Arc<Config>,
    site: Arc<Client>,
}

pub fn routes(cfg: Arc<Config>, site: Arc<Client>) -> Router {
    Router::new()
        .route("/provider/info", get(info))
        .route("/search", get(search))
        .route("/book/{id}", get(lookup))
        .with_state(AppState { cfg, site })
}

async fn info(State(state): State<AppState>) -> Response {
    Json(json!({
        "name": "GraphicAudio",
        "version": VERSION,
        "specVersion": "1",
        "authenticationRequired": state.cfg.auth_token.is_some(),
        // Proposed extensions.
        "key": "graphicaudio",
        "mediaTypes": ["audiobook"],
        "lookup": true,
        "supportedFields": [
            "title", "subtitle", "description", "cover", "authors", "narrators", "publisher",
            "publishedYear", "language", "seriesName", "seriesIndex", "genres", "isbn13", "duration"
        ],
        "searchParams": ["author", "limit"],
        "idMaxLength": MAX_ID_LENGTH,
        "coverShape": "square",
        "color": "#c8102e",
        "rateLimit": { "maxConcurrency": 1, "minIntervalMs": state.cfg.interval.as_millis() as u64 },
    }))
    .into_response()
}

async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Some(denied) = deny_unauthorized(&state, &headers) {
        return denied;
    }
    let raw = q.get("query").map(|v| v.trim()).unwrap_or_default();
    if raw.is_empty() {
        return error(StatusCode::BAD_REQUEST, "query is required");
    }
    // GraphicAudio sells audio only; a printed-book search gets no answer, not a wrong one.
    if matches!(q.get("mediaType").map(String::as_str), Some("book" | "comic")) {
        return Json(json!({ "matches": [] })).into_response();
    }
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .map_or(DEFAULT_LIMIT, |n| n.clamp(1, MAX_LIMIT));
    let author = q.get("author").map(|v| v.trim()).unwrap_or_default();

    let result = tokio::time::timeout(REQUEST_TIMEOUT, find(&state.site, raw, author, limit)).await;
    match result {
        Ok(Ok(matches)) => Json(json!({ "matches": matches })).into_response(),
        Ok(Err(err)) => upstream_error("search", err),
        Err(_) => upstream_error("search", UpstreamError::Failed("timed out".into())),
    }
}

async fn find(
    site: &Client,
    raw: &str,
    author: &str,
    limit: usize,
) -> Result<Vec<BookMetadata>, UpstreamError> {
    let query = clean_query(raw);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let part = wanted_part(raw);
    let page = site.search_page(&query).await?;
    let mut ids: Vec<String> = product_links(&page)
        .into_iter()
        .filter(|id| slug_matches(id, &query))
        .collect();
    // Books are searched by title, which the slug carries; a query that is only a series name
    // matches every title in it, so favour the part that was asked for and keep the fetches few.
    if let Some(part) = part {
        let part = part.to_string();
        ids.sort_by_key(|id| {
            // Slugs end `-<part>-of-<total>`.
            let mut tail = id.rsplit('-');
            let (_total, of, number) = (tail.next(), tail.next(), tail.next());
            !(of == Some("of") && number == Some(part.as_str()))
        });
    }
    ids.truncate(MAX_FETCHES);

    let mut found = Vec::new();
    for id in ids {
        match site.product_page(&id).await {
            Ok(html) => found.extend(map_product(&html, &id)),
            Err(UpstreamError::NotFound) => {}
            Err(err) => return Err(err),
        }
    }
    let mut ranked = rank(found, &query, author, part);
    ranked.truncate(limit);
    Ok(ranked)
}

async fn lookup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Some(denied) = deny_unauthorized(&state, &headers) {
        return denied;
    }
    let valid = !id.is_empty()
        && id.len() <= MAX_ID_LENGTH
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !valid {
        return error(StatusCode::NOT_FOUND, "Not found");
    }
    let fetched = tokio::time::timeout(REQUEST_TIMEOUT, state.site.product_page(&id)).await;
    match fetched {
        Ok(Ok(html)) => match map_product(&html, &id) {
            Some(book) => Json(book).into_response(),
            None => error(StatusCode::NOT_FOUND, "Not found"),
        },
        Ok(Err(UpstreamError::NotFound)) => error(StatusCode::NOT_FOUND, "Not found"),
        Ok(Err(err)) => upstream_error("lookup", err),
        Err(_) => upstream_error("lookup", UpstreamError::Failed("timed out".into())),
    }
}

/// Status codes are part of the contract: BookOrbit backs off on 429 and treats 5xx as a failure
/// it may answer from its cache, never as "no results".
fn upstream_error(what: &str, err: UpstreamError) -> Response {
    match err {
        UpstreamError::RateLimited { retry_after_seconds } => {
            let mut res = error(StatusCode::TOO_MANY_REQUESTS, "rate limited");
            res.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(retry_after_seconds));
            res
        }
        UpstreamError::NotFound => error(StatusCode::NOT_FOUND, "Not found"),
        UpstreamError::Failed(message) => {
            eprintln!("[provider] [{what}] failed: {message}");
            error(StatusCode::BAD_GATEWAY, "source unavailable")
        }
    }
}

fn deny_unauthorized(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let token = state.cfg.auth_token.as_deref()?;
    let given = headers
        .get(header::AUTHORIZATION)
        .map(HeaderValue::as_bytes)
        .unwrap_or_default();
    let matches = given.len() == token.len()
        && given
            .iter()
            .zip(token.as_bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0;
    (!matches).then(|| error(StatusCode::UNAUTHORIZED, "Unauthorized"))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}
