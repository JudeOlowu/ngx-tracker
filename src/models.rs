use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Sector {
    Energy,
    Fintech,
    Agriculture,
    Finance,
    Healthcare,
    Other(String),
}

impl Sector {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "energy" | "oil & gas" | "petroleum" => Sector::Energy,
            "fintech" | "financial technology" | "technology" => Sector::Fintech,
            "agriculture" | "agric" | "agro" => Sector::Agriculture,
            "finance" | "banking" | "insurance" | "financial services" => Sector::Finance,
            "healthcare" | "health" | "pharmaceutical" | "pharma" => Sector::Healthcare,
            other => Sector::Other(other.to_string()),
        }
    }

    pub fn display(&self) -> &str {
        match self {
            Sector::Energy => "Energy",
            Sector::Fintech => "Fintech",
            Sector::Agriculture => "Agriculture",
            Sector::Finance => "Finance",
            Sector::Healthcare => "Healthcare",
            Sector::Other(s) => s.as_str(),
        }
    }

    pub fn is_target(&self) -> bool {
        !matches!(self, Sector::Other(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockPrice {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stock {
    pub ticker: String,
    pub company_name: String,
    pub sector: Sector,
    pub current_price: f64,
    pub price_3m_ago: f64,
    pub percent_change: f64,
    pub avg_volume: f64,
    pub market_cap: Option<f64>,
    pub history: Vec<StockPrice>,
}

impl Stock {
    pub fn calculate_metrics(&mut self) {
        if let (Some(latest), Some(oldest)) = (self.history.last(), self.history.first()) {
            self.current_price = latest.close;
            self.price_3m_ago = oldest.close;
            if oldest.close > 0.0 {
                self.percent_change =
                    ((latest.close - oldest.close) / oldest.close) * 100.0;
            }
            let total_vol: u64 = self.history.iter().map(|h| h.volume).sum();
            self.avg_volume = total_vol as f64 / self.history.len() as f64;
        }
    }
}

#[derive(Debug)]
pub struct ScreenerResult {
    pub top_stocks: Vec<Stock>,
    pub best_sector: String,
    pub worst_sector: String,
    pub avg_gain: f64,
}