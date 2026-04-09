use colored::*;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Attribute, Cell, CellAlignment,
    Color as TColor, ContentArrangement, Table,
};

use crate::models::ScreenerResult;

pub fn render(result: &ScreenerResult, top_n: usize) {
    println!(
        "\n{}",
        format!(
            "  🇳🇬  NGX TOP {} PERFORMERS — LAST 3 MONTHS  🇳🇬",
            top_n
        )
        .bold()
        .bright_green()
    );

    println!(
        "  {}",
        "Sectors: Energy | Fintech | Agriculture | Finance | Healthcare"
            .dimmed()
    );
    println!("{}", "─".repeat(110).dimmed());

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Rank")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan),
            Cell::new("Ticker")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan),
            Cell::new("Company")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan),
            Cell::new("Sector")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan),
            Cell::new("Price Now (₦)")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan)
                .set_alignment(CellAlignment::Right),
            Cell::new("3M Ago (₦)")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan)
                .set_alignment(CellAlignment::Right),
            Cell::new("% Change")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan)
                .set_alignment(CellAlignment::Right),
            Cell::new("Avg Vol")
                .add_attribute(Attribute::Bold)
                .fg(TColor::Cyan)
                .set_alignment(CellAlignment::Right),
        ]);

    for (i, stock) in result.top_stocks.iter().enumerate() {
        let rank = format!("{}", i + 1);
        let pct = stock.percent_change;
        let change_str = format!("{:+.2}%", pct);

        let (pct_cell, rank_color) = if pct >= 0.0 {
            (
                Cell::new(&change_str).fg(TColor::Green),
                TColor::Green,
            )
        } else {
            (
                Cell::new(&change_str).fg(TColor::Red),
                TColor::Red,
            )
        };

        let sector_color = match stock.sector.display() {
            "Energy" => TColor::Yellow,
            "Fintech" => TColor::Magenta,
            "Agriculture" => TColor::Green,
            "Finance" => TColor::Blue,
            "Healthcare" => TColor::Cyan,
            _ => TColor::White,
        };

        let vol_str = format_volume(stock.avg_volume);

        table.add_row(vec![
            Cell::new(&rank).fg(rank_color),
            Cell::new(&stock.ticker).add_attribute(Attribute::Bold),
            Cell::new(&stock.company_name),
            Cell::new(stock.sector.display()).fg(sector_color),
            Cell::new(format!("{:.2}", stock.current_price))
                .set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2}", stock.price_3m_ago))
                .set_alignment(CellAlignment::Right),
            pct_cell.set_alignment(CellAlignment::Right),
            Cell::new(&vol_str).set_alignment(CellAlignment::Right),
        ]);
    }

    println!("{table}");

    // Summary footer
    println!("\n{}", "  ── SUMMARY ──".bold().bright_cyan());
    println!(
        "  🏆 Best Sector   : {}",
        result.best_sector.bright_green().bold()
    );
    println!(
        "  📉 Worst Sector  : {}",
        result.worst_sector.bright_red().bold()
    );
    println!(
        "  📊 Avg Gain (Top {}): {}",
        top_n,
        format!("{:+.2}%", result.avg_gain).bright_yellow().bold()
    );
    println!();
}

fn format_volume(vol: f64) -> String {
    if vol >= 1_000_000.0 {
        format!("{:.1}M", vol / 1_000_000.0)
    } else if vol >= 1_000.0 {
        format!("{:.1}K", vol / 1_000.0)
    } else {
        format!("{:.0}", vol)
    }
}