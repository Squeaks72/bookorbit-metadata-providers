use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A product page is about 320 KB; a series page a little less.
const MAX_BODY_BYTES: usize = 4 << 20;
/// Longer than this in the pacer's queue and the caller is told to come back later instead.
const MAX_QUEUE_WAIT: Duration = Duration::from_secs(20);
/// Release data on a finished recording does not change; a day keeps repeat scans off the site.
const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);
const CACHE_LIMIT: usize = 512;

#[derive(Debug)]
pub enum UpstreamError {
    NotFound,
    RateLimited { retry_after_seconds: u64 },
    Failed(String),
}

pub struct Client {
    base: String,
    http: reqwest::Client,
    next: Mutex<Instant>,
    interval: Duration,
    pages: Mutex<HashMap<String, (Instant, String, String)>>,
}

impl Client {
    pub fn new(base: &str, user_agent: &str, interval: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(4))
            .user_agent(user_agent)
            .build()
            .expect("could not build the HTTP client");
        Self {
            base: base.trim_end_matches('/').to_string(),
            http,
            next: Mutex::new(Instant::now()),
            interval,
            pages: Mutex::new(HashMap::new()),
        }
    }

    /// The page a catalogue search lands on, with the URL it ended at: the matching series page
    /// when the site has one, the product's own page for a single hit, else a result list.
    pub async fn search_page(&self, query: &str) -> Result<(String, String), UpstreamError> {
        self.get(&format!("/catalogsearch/result/?q={}", encode(query)))
            .await
    }

    pub async fn product_page(&self, id: &str) -> Result<String, UpstreamError> {
        self.get(&format!("/{id}.html")).await.map(|(_, page)| page)
    }

    async fn get(&self, path: &str) -> Result<(String, String), UpstreamError> {
        if let Some(page) = self.cached(path) {
            return Ok(page);
        }
        self.wait().await?;
        let mut res = self
            .http
            .get(format!("{}{path}", self.base))
            .header("Accept", "text/html")
            .send()
            .await
            .map_err(|error| UpstreamError::Failed(error.to_string()))?;
        let url = res.url().to_string();
        match res.status().as_u16() {
            404 => return Err(UpstreamError::NotFound),
            429 | 503 => {
                let retry_after_seconds = res
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .filter(|seconds| *seconds > 0)
                    .unwrap_or(60);
                return Err(UpstreamError::RateLimited { retry_after_seconds });
            }
            200..=299 => {}
            status => {
                return Err(UpstreamError::Failed(format!(
                    "graphicaudio.net answered {status}"
                )))
            }
        }
        let mut body = Vec::new();
        while let Some(chunk) = res
            .chunk()
            .await
            .map_err(|error| UpstreamError::Failed(error.to_string()))?
        {
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(UpstreamError::Failed("page too large".into()));
            }
            body.extend_from_slice(&chunk);
        }
        let page = String::from_utf8_lossy(&body).into_owned();
        self.store(path, &url, &page);
        Ok((url, page))
    }

    fn cached(&self, key: &str) -> Option<(String, String)> {
        let pages = self.pages.lock().expect("cache lock poisoned");
        pages
            .get(key)
            .filter(|(at, _, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, url, page)| (url.clone(), page.clone()))
    }

    fn store(&self, key: &str, url: &str, page: &str) {
        let mut pages = self.pages.lock().expect("cache lock poisoned");
        if pages.len() >= CACHE_LIMIT {
            pages.retain(|_, (at, _, _)| at.elapsed() < CACHE_TTL);
        }
        if pages.len() >= CACHE_LIMIT {
            pages.clear();
        }
        pages.insert(
            key.to_string(),
            (Instant::now(), url.to_string(), page.to_string()),
        );
    }

    /// Spaces request starts at least `interval` apart, across all callers.
    async fn wait(&self) -> Result<(), UpstreamError> {
        if self.interval.is_zero() {
            return Ok(());
        }
        let start = {
            let mut next = self.next.lock().expect("pacer lock poisoned");
            let now = Instant::now();
            let start = (*next).max(now);
            let wait = start - now;
            if wait > MAX_QUEUE_WAIT {
                return Err(UpstreamError::RateLimited {
                    retry_after_seconds: wait.as_secs_f64().ceil() as u64,
                });
            }
            *next = start + self.interval;
            start
        };
        tokio::time::sleep_until(tokio::time::Instant::from_std(start)).await;
        Ok(())
    }
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_a_query() {
        assert_eq!(encode("Alloy of Law & more"), "Alloy+of+Law+%26+more");
    }
}
