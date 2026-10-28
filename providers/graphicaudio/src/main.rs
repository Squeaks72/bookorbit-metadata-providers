mod mapping;
mod server;
mod site;

use std::sync::Arc;
use std::time::Duration;

pub const VERSION: &str = "0.1.0-tim.2";

pub struct Config {
    pub auth_token: Option<String>,
    /// Gap between requests to graphicaudio.net. A search fetches the few product pages that can
    /// match, so this is what keeps a library scan polite.
    pub interval: Duration,
}

#[tokio::main]
async fn main() {
    let port = env("PORT", "8080");
    let site = env("GRAPHICAUDIO_URL", "https://www.graphicaudio.net");
    let user_agent = env(
        "USER_AGENT",
        &format!("bookorbit-graphicaudio-provider/{VERSION}"),
    );
    let cfg = Config {
        auth_token: std::env::var("AUTH_TOKEN")
            .ok()
            .filter(|token| !token.is_empty()),
        interval: Duration::from_millis(
            std::env::var("REQUEST_INTERVAL_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(750),
        ),
    };
    let client = site::Client::new(&site, &user_agent, cfg.interval);
    let app = server::routes(Arc::new(cfg), Arc::new(client));

    let listener = tokio::net::TcpListener::bind((
        "0.0.0.0",
        port.parse::<u16>().expect("PORT must be a port number"),
    ))
    .await
    .expect("could not bind the port");
    eprintln!("[provider] [start] port={port} site={site} version={VERSION} - listening");
    axum::serve(listener, app).await.expect("server stopped");
}

fn env(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}
