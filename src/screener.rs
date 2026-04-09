use crate::models::{ScreenerResult, Stock};
use std::collections::HashMap;

pub fn screen(mut stocks: Vec<Stock>, top_n: usize) -> ScreenerResult {
    // Sort descending by percent change
    stocks.sort_by(|a, b| {
        b.percent_change
            .partial_cmp(&a.percent_change)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_stocks: Vec<Stock> = stocks.into_iter().take(top_n).collect();

    // Per-sector average gains
    let mut sector_gains: HashMap<String, Vec<f64>> = HashMap::new();
    for stock in &top_stocks {
        sector_gains
            .entry(stock.sector.display().to_string())
            .or_default()
            .push(stock.percent_change);
    }

    let sector_avgs: Vec<(String, f64)> = sector_gains
        .iter()
        .map(|(s, gains)| {
            let avg = gains.iter().sum::<f64>() / gains.len() as f64;
            (s.clone(), avg)
        })
        .collect();

    let best_sector = sector_avgs
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(s, _)| s.clone())
        .unwrap_or_default();

    let worst_sector = sector_avgs
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(s, _)| s.clone())
        .unwrap_or_default();

    let avg_gain = if !top_stocks.is_empty() {
        top_stocks.iter().map(|s| s.percent_change).sum::<f64>() / top_stocks.len() as f64
    } else {
        0.0
    };

    ScreenerResult {
        top_stocks,
        best_sector,
        worst_sector,
        avg_gain,
    }
}