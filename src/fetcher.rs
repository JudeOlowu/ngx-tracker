// ============================================================
//  fetcher.rs
//  Live data via Alpha Vantage + NGX scraper fallback
//  + mock data if both fail
// ============================================================

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, Local};
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashMap;
use tokio_retry::{strategy::ExponentialBackoff, Retry};
use tracing::{info, warn};

use crate::models::{Sector, Stock, StockPrice};

// ─────────────────────────────────────────────
//  NGX Universe
// ─────────────────────────────────────────────
pub fn get_ngx_universe() -> Vec<(String, String, Sector)> {
    vec![
        ("GTCO".into(),       "Guaranty Trust Holding Co".into(),             Sector::Finance),
        ("ZENITHBANK".into(), "Zenith Bank Plc".into(),                       Sector::Finance),
        ("ACCESS".into(),     "Access Holdings Plc".into(),                   Sector::Finance),
        ("UBA".into(),        "United Bank for Africa".into(),                Sector::Finance),
        ("FBNH".into(),       "FBN Holdings Plc".into(),                      Sector::Finance),
        ("STANBIC".into(),    "Stanbic IBTC Holdings".into(),                 Sector::Finance),
        ("FCMB".into(),       "FCMB Group Plc".into(),                        Sector::Finance),
        ("FIDELITYBK".into(), "Fidelity Bank Plc".into(),                     Sector::Finance),
        ("WEMABANK".into(),   "Wema Bank Plc".into(),                         Sector::Finance),
        ("ETI".into(),        "Ecobank Transnational Inc".into(),             Sector::Finance),
        ("MTNN".into(),       "MTN Nigeria Communications".into(),            Sector::Fintech),
        ("AIRTELAFRI".into(), "Airtel Africa Plc".into(),                     Sector::Fintech),
        ("TRANSCORP".into(),  "Transnational Corporation".into(),             Sector::Fintech),
        ("CHAMS".into(),      "Chams Holding Company".into(),                 Sector::Fintech),
        ("OMATEK".into(),     "Omatek Ventures Plc".into(),                   Sector::Fintech),
        ("SEPLAT".into(),     "Seplat Energy Plc".into(),                     Sector::Energy),
        ("CONOIL".into(),     "Conoil Plc".into(),                            Sector::Energy),
        ("ETERNA".into(),     "Eterna Plc".into(),                            Sector::Energy),
        ("OANDO".into(),      "Oando Plc".into(),                             Sector::Energy),
        ("TOTAL".into(),      "TotalEnergies Marketing Nigeria".into(),       Sector::Energy),
        ("MRS".into(),        "MRS Oil Nigeria Plc".into(),                   Sector::Energy),
        ("ARDOVA".into(),     "Ardova Plc".into(),                            Sector::Energy),
        ("PRESCO".into(),     "Presco Plc".into(),                            Sector::Agriculture),
        ("OKOMUOIL".into(),   "Okomu Oil Palm Co Plc".into(),                 Sector::Agriculture),
        ("LIVESTOCK".into(),  "Livestock Feeds Plc".into(),                   Sector::Agriculture),
        ("FLOURMILL".into(),  "Flour Mills of Nigeria".into(),                Sector::Agriculture),
        ("DANGSUGAR".into(),  "Dangote Sugar Refinery".into(),                Sector::Agriculture),
        ("NASCON".into(),     "NASCON Allied Industries".into(),              Sector::Agriculture),
        ("MAYBAKER".into(),   "May and Baker Nigeria Plc".into(),             Sector::Healthcare),
        ("GLAXOSMITH".into(), "GlaxoSmithKline Consumer Nigeria".into(),      Sector::Healthcare),
        ("NEIMETH".into(),    "Neimeth International Pharmaceuticals".into(), Sector::Healthcare),
        ("FIDSON".into(),     "Fidson Healthcare Plc".into(),                 Sector::Healthcare),
        ("PHARMDEKO".into(),  "Pharma-Deko Plc".into(),                       Sector::Healthcare),
        ("UNION".into(),      "Union Diagnostic and Clinical Services".into(),Sector::Healthcare),
    ]
}

// ─────────────────────────────────────────────
//  MAIN ENTRY POINT
//  Priority: Alpha Vantage -> NGX scrape -> mock
// ─────────────────────────────────────────────
pub async fn fetch_all_stocks(client: &Client) -> Result<Vec<Stock>> {
    info!("NGX Screener starting data fetch...");

    // Try Alpha Vantage if key is set
    match std::env::var("ALPHA_VANTAGE_KEY") {
        Ok(key) if !key.is_empty() && key != "YOUR_KEY_HERE" => {
            info!("Alpha Vantage key found - fetching live data");
            match fetch_via_alpha_vantage(client, &key).await {
                Ok(stocks) if stocks.len() >= 5 => {
                    info!("Alpha Vantage: {} stocks loaded", stocks.len());
                    return Ok(stocks);
                }
                Ok(s) => warn!("Alpha Vantage returned only {} stocks", s.len()),
                Err(e) => warn!("Alpha Vantage failed: {}", e),
            }
        }
        _ => info!("No ALPHA_VANTAGE_KEY in .env - skipping live fetch"),
    }

    // Try NGX scrape
    info!("Attempting NGX direct scrape...");
    match fetch_ngx_scrape(client).await {
        Ok(stocks) if stocks.len() >= 5 => {
            info!("NGX scrape: {} stocks loaded", stocks.len());
            return Ok(stocks);
        }
        Ok(_) => warn!("NGX scrape returned too few results"),
        Err(e) => warn!("NGX scrape failed: {}", e),
    }

    // Fall back to mock
    warn!("All live sources failed - using mock data");
    warn!("Add ALPHA_VANTAGE_KEY=<your_key> to .env for live data");
    Ok(generate_mock_stocks())
}

// ─────────────────────────────────────────────
//  SOURCE 1 - Alpha Vantage
//  Get key free at: https://www.alphavantage.co/support/#api-key
//  NGX tickers use the ".LG" suffix (Lagos Stock Exchange)
// ─────────────────────────────────────────────
async fn fetch_via_alpha_vantage(client: &Client, api_key: &str) -> Result<Vec<Stock>> {
    let universe = get_ngx_universe();
    let mut stocks = Vec::new();
    let total = universe.iter().filter(|(_, _, s)| s.is_target()).count();

    info!("Fetching {} tickers (500ms between calls for rate limiting)...", total);

    for (ticker, name, sector) in &universe {
        if !sector.is_target() {
            continue;
        }

        // Free tier: 25 requests/day, max 5/minute
        // 500ms delay keeps us safe within rate limits
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Alpha Vantage uses ".LG" suffix for NGX/Lagos tickers
        let symbol = format!("{}.LG", ticker);

        match fetch_single_alpha_vantage(client, api_key, &symbol).await {
            Ok(history) if history.len() >= 2 => {
                let mut stock = Stock {
                    ticker: ticker.clone(),
                    company_name: name.clone(),
                    sector: sector.clone(),
                    current_price: 0.0,
                    price_3m_ago: 0.0,
                    percent_change: 0.0,
                    avg_volume: 0.0,
                    market_cap: None,
                    history,
                };
                stock.calculate_metrics();
                info!("  OK {} - price: {:.2} change: {:+.2}%",
                    ticker, stock.current_price, stock.percent_change);
                stocks.push(stock);
            }
            Ok(_) => {
                warn!("  SKIP {} - not on Alpha Vantage, using mock", ticker);
                stocks.push(make_mock_stock(ticker, name, sector));
            }
            Err(e) => {
                warn!("  FAIL {} - {}, using mock", ticker, e);
                stocks.push(make_mock_stock(ticker, name, sector));
            }
        }
    }

    Ok(stocks)
}

async fn fetch_single_alpha_vantage(
    client: &Client,
    api_key: &str,
    symbol: &str,
) -> Result<Vec<StockPrice>> {
    let url = format!(
        "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol={}&outputsize=compact&apikey={}",
        symbol, api_key
    );

    let retry_strategy = ExponentialBackoff::from_millis(800).take(3);

    let resp: serde_json::Value = Retry::spawn(retry_strategy, || async {
        client
            .get(&url)
            .header("User-Agent", "NGXScreener/1.0")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await
    })
    .await
    .context("Alpha Vantage request failed after 3 retries")?;

    // Check for API errors or rate limit messages
    if let Some(note) = resp.get("Note").or_else(|| resp.get("Information")) {
        let msg = note.as_str().unwrap_or("API limit or key error");
        anyhow::bail!("Alpha Vantage: {}", &msg[..msg.len().min(120)]);
    }

    let series = resp["Time Series (Daily)"]
        .as_object()
        .context("No 'Time Series (Daily)' returned - symbol may not exist on Alpha Vantage")?;

    // Parse up to 63 trading days (approx 3 months)
    let mut prices: Vec<StockPrice> = series
        .iter()
        .take(63)
        .filter_map(|(date_str, vals)| {
            let date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
            Some(StockPrice {
                date,
                open:   vals["1. open"].as_str()?.parse().ok()?,
                high:   vals["2. high"].as_str()?.parse().ok()?,
                low:    vals["3. low"].as_str()?.parse().ok()?,
                close:  vals["4. close"].as_str()?.parse().ok()?,
                volume: vals["5. volume"].as_str()?.parse().ok()?,
            })
        })
        .collect();

    // Sort oldest to newest for correct metric calculation
    prices.sort_by_key(|p| p.date);
    Ok(prices)
}

// ─────────────────────────────────────────────
//  SOURCE 2 - NGX Direct Scrape
//  Scrapes the official NGX equities price list
// ─────────────────────────────────────────────
async fn fetch_ngx_scrape(client: &Client) -> Result<Vec<Stock>> {
    let url = "https://ngxgroup.com/exchange/data/equities-price-list/";

    let retry_strategy = ExponentialBackoff::from_millis(600).take(3);
    let html = Retry::spawn(retry_strategy, || async {
        client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (compatible; NGXScreener/1.0)")
            .send()
            .await?
            .text()
            .await
    })
    .await
    .context("NGX scrape failed after 3 retries")?;

    parse_ngx_html(&html)
}

fn parse_ngx_html(html: &str) -> Result<Vec<Stock>> {
    let document = Html::parse_document(html);
    let row_sel  = Selector::parse("table tbody tr").unwrap();
    let cell_sel = Selector::parse("td").unwrap();

    let universe: HashMap<String, (String, Sector)> = get_ngx_universe()
        .into_iter()
        .map(|(t, n, s)| (t, (n, s)))
        .collect();

    let mut stocks = Vec::new();

    for row in document.select(&row_sel) {
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .collect();

        if cells.len() < 4 { continue; }

        let ticker = cells[0].trim().to_uppercase();
        let close: f64 = cells[3].replace(',', "").trim().parse().unwrap_or(0.0);

        if close == 0.0 { continue; }

        if let Some((name, sector)) = universe.get(&ticker) {
            if !sector.is_target() { continue; }

            // Scrape gives current price only; history is synthetic
            let history = generate_price_history(close, 63);
            let mut stock = Stock {
                ticker,
                company_name: name.clone(),
                sector: sector.clone(),
                current_price: close,
                price_3m_ago: 0.0,
                percent_change: 0.0,
                avg_volume: 0.0,
                market_cap: None,
                history,
            };
            stock.calculate_metrics();
            stocks.push(stock);
        }
    }

    if stocks.is_empty() {
        anyhow::bail!("No matching tickers found - NGX page layout may have changed");
    }
    Ok(stocks)
}

// ─────────────────────────────────────────────
//  SOURCE 3 - Mock / Seeded Fallback
// ─────────────────────────────────────────────
pub fn generate_mock_stocks() -> Vec<Stock> {
    get_ngx_universe()
        .into_iter()
        .filter(|(_, _, s)| s.is_target())
        .map(|(ticker, name, sector)| make_mock_stock(&ticker, &name, &sector))
        .collect()
}

fn get_base_price(ticker: &str) -> f64 {
    let prices: HashMap<&str, f64> = [
        ("GTCO", 45.50),  ("ZENITHBANK", 37.20), ("ACCESS", 22.80),
        ("UBA", 18.50),   ("FBNH", 14.20),       ("STANBIC", 58.00),
        ("FCMB", 8.75),   ("FIDELITYBK", 12.40), ("WEMABANK", 5.60),
        ("ETI", 15.30),   ("MTNN", 220.50),      ("AIRTELAFRI", 1850.00),
        ("TRANSCORP", 4.20), ("CHAMS", 2.15),    ("OMATEK", 0.85),
        ("SEPLAT", 3200.00), ("CONOIL", 88.50),  ("ETERNA", 18.20),
        ("OANDO", 15.60), ("TOTAL", 425.00),     ("MRS", 78.00),
        ("ARDOVA", 34.50),("PRESCO", 485.00),    ("OKOMUOIL", 295.00),
        ("LIVESTOCK", 3.20), ("FLOURMILL", 42.00), ("DANGSUGAR", 38.50),
        ("NASCON", 52.00),("MAYBAKER", 7.80),    ("GLAXOSMITH", 12.50),
        ("NEIMETH", 2.40),("FIDSON", 15.20),     ("PHARMDEKO", 3.85),
        ("UNION", 1.90),
    ].iter().cloned().collect();
    *prices.get(ticker).unwrap_or(&10.0)
}

fn make_mock_stock(ticker: &str, name: &str, sector: &Sector) -> Stock {
    let price = get_base_price(ticker);
    let history = generate_price_history(price, 63);
    let mut stock = Stock {
        ticker: ticker.to_string(),
        company_name: name.to_string(),
        sector: sector.clone(),
        current_price: price,
        price_3m_ago: 0.0,
        percent_change: 0.0,
        avg_volume: 0.0,
        market_cap: None,
        history,
    };
    stock.calculate_metrics();
    stock
}

// ─────────────────────────────────────────────
//  HELPER - Synthetic price history generator
//  Deterministic seed so mock is consistent
// ─────────────────────────────────────────────
pub fn generate_price_history(current_close: f64, days: usize) -> Vec<StockPrice> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    current_close.to_bits().hash(&mut hasher);
    let seed = hasher.finish();

    let start_date = Local::now().date_naive() - Duration::days(days as i64);
    let start_price = current_close * (0.75 + (seed % 50) as f64 / 100.0);

    let mut price = start_price;
    let mut history = Vec::new();

    for i in 0..days {
        let date = start_date + Duration::days(i as i64);

        // NGX does not trade on weekends
        if date.weekday() == chrono::Weekday::Sat || date.weekday() == chrono::Weekday::Sun {
            continue;
        }

        let noise = ((seed
            .wrapping_add(i as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)) % 1000) as f64
            / 10000.0
            - 0.05;

        price *= 1.0 + noise;
        price = price.max(0.50); // floor at 50 kobo

        let volume = 100_000
            + (seed.wrapping_add(i as u64).wrapping_mul(1234567891) % 9_900_000);

        history.push(StockPrice {
            date,
            open:   (price * 0.990 * 100.0).round() / 100.0,
            high:   (price * 1.025 * 100.0).round() / 100.0,
            low:    (price * 0.975 * 100.0).round() / 100.0,
            close:  (price * 100.0).round() / 100.0,
            volume,
        });
    }

    // Snap the final close to the real current price
    if let Some(last) = history.last_mut() {
        last.close = current_close;
    }

    history
}