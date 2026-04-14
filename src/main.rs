mod display;
mod exporter;
mod fetcher;
mod insider;
mod models;
mod screener;

use anyhow::Result;
use axum::{
    extract::State,
    http::{header, HeaderValue, Method, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use reqwest::Client;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    set_header::SetResponseHeaderLayer,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub html:            Arc<RwLock<String>>,
    pub json:            Arc<RwLock<String>>,
    pub insider_alerts:  Arc<RwLock<Vec<insider::InsiderAlert>>>,
    pub request_count:   Arc<std::sync::atomic::AtomicU64>,
}

#[derive(Parser, Debug)]
#[command(name = "ngx_screener", about = "NGX Radar — Nigerian Stock Exchange")]
struct Cli {
    #[arg(long)]
    serve: bool,
    #[arg(short, long, default_value_t = 30)]
    top: usize,
    #[arg(short, long)]
    sector: Option<String>,
    #[arg(short, long, default_value_t = false)]
    export: bool,
    #[arg(long, default_value = "change")]
    sort_by: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli   = Cli::parse();
    let port  = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let serve_mode = cli.serve
        || std::env::var("PORT").is_ok()
        || std::env::var("RAILWAY_ENVIRONMENT").is_ok()
        || std::env::var("RENDER").is_ok();

    if serve_mode {
        run_server(&port, cli.top).await
    } else {
        run_once(&cli).await
    }
}

// ── ONE-SHOT MODE ─────────────────────────────────────────────
async fn run_once(cli: &Cli) -> Result<()> {
    info!("NGX Screener — one-shot mode");
    let client = Client::builder().timeout(Duration::from_secs(20)).build()?;
    let mut stocks = fetcher::fetch_all_stocks(&client).await?;

    if let Some(ref sf) = cli.sector {
        let target = models::Sector::from_str(sf);
        stocks.retain(|s| s.sector == target);
    }
    match cli.sort_by.as_str() {
        "volume" => stocks.sort_by(|a, b| b.avg_volume.partial_cmp(&a.avg_volume).unwrap_or(std::cmp::Ordering::Equal)),
        "price"  => stocks.sort_by(|a, b| b.current_price.partial_cmp(&a.current_price).unwrap_or(std::cmp::Ordering::Equal)),
        _        => {}
    }

    let result = screener::screen(stocks, cli.top);
    display::render(&result, cli.top);

    match exporter::export_html(&result.top_stocks) {
        Ok(p)  => {
            println!("\n  ================================================");
            println!("  SUCCESS — open this file in your browser:");
            println!("  {}", p);
            println!("  ================================================\n");
        }
        Err(e) => eprintln!("  WARNING: {}", e),
    }

    if cli.export {
        if let Ok(p) = exporter::export_csv(&result.top_stocks) {
            println!("  CSV -> {}", p);
        }
    }
    Ok(())
}

// ── SERVER MODE (Render / Railway) ────────────────────────────
async fn run_server(port: &str, top: usize) -> Result<()> {
    info!("NGX Radar — server mode on port {}", port);

    let initial_html = generate_html(top).await;

    let state = AppState {
        html:           Arc::new(RwLock::new(initial_html)),
        json:           Arc::new(RwLock::new("{}".to_string())),
        insider_alerts: Arc::new(RwLock::new(insider::get_known_insider_transactions())),
        request_count:  Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    // Background: refresh stock data every 30 seconds
    let sc = state.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let html = generate_html(top).await;
            if let Ok(mut w) = sc.html.write() {
                *w = html;
                info!("Stock data refreshed");
            }
        }
    });

    // Background: refresh insider alerts every hour
    let sc2 = state.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match insider::fetch_insider_alerts().await {
                Ok(alerts) => {
                    if let Ok(mut w) = sc2.insider_alerts.write() {
                        *w = alerts;
                        info!("Insider alerts refreshed");
                    }
                }
                Err(e) => eprintln!("Insider fetch error: {}", e),
            }
        }
    });

    // Security headers
    let security_layer = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
                 img-src 'self' https://logo.clearbit.com https://flagcdn.com data:; \
                 connect-src 'none'; frame-ancestors 'none';"
            ),
        ));

    let cors = CorsLayer::new()
        .allow_methods([Method::GET])
        .allow_origin(Any);

    let app = Router::new()
        .route("/",         get(serve_dashboard))
        .route("/api/data", get(serve_json))
        .route("/health",   get(health_check))
        .fallback(handler_404)
        .with_state(state)
        .layer(security_layer)
        .layer(cors);

    let addr = format!("0.0.0.0:{}", port);
    info!("Listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ── ROUTE HANDLERS ────────────────────────────────────────────

async fn serve_dashboard(State(state): State<AppState>) -> impl IntoResponse {
    state.request_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let html = state.html.read().unwrap().clone();
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL,  "no-cache, must-revalidate"),
            (header::CONTENT_TYPE,   "text/html; charset=utf-8"),
        ],
        html,
    )
}

async fn serve_json(State(state): State<AppState>) -> impl IntoResponse {
    let json = state.json.read().unwrap().clone();
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "public, max-age=30"),
            (header::CONTENT_TYPE,  "application/json"),
        ],
        json,
    )
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.request_count.load(std::sync::atomic::Ordering::Relaxed);
    format!("OK — {} requests served", count)
}

async fn handler_404(uri: Uri) -> impl IntoResponse {
    let path = uri.path().to_owned();
    let html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"/><title>Not Found — NGX Radar</title>
<style>body{{background:#080b12;color:#e2e8f0;font-family:sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;text-align:center}}
.box{{max-width:400px;padding:20px}}h1{{font-size:22px;margin-bottom:10px}}p{{color:#64748b;font-size:14px;margin-bottom:24px}}
a{{background:linear-gradient(135deg,#22c55e,#16a34a);color:#fff;text-decoration:none;padding:11px 22px;border-radius:8px;font-weight:700}}</style>
</head>
<body><div class="box"><div style="font-size:48px">🇳🇬</div>
<h1>Page Not Found</h1>
<p>The page <code style="background:#1e2535;padding:2px 6px;border-radius:3px">{path}</code> doesn't exist.</p>
<a href="/">Go to Dashboard →</a>
</div></body></html>"#);
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
}

async fn generate_html(top: usize) -> String {
    let client = match Client::builder().timeout(Duration::from_secs(20)).build() {
        Ok(c)  => c,
        Err(_) => return exporter::fallback_html(),
    };
    match fetcher::fetch_all_stocks(&client).await {
        Ok(stocks) => {
            let result = screener::screen(stocks, top);
            exporter::build_html_string(&result.top_stocks)
        }
        Err(e) => {
            eprintln!("Fetch error: {}", e);
            exporter::fallback_html()
        }
    }
}
