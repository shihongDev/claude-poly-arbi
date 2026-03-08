use anyhow::Context;
use arb_core::Opportunity;
use serde::Serialize;
use std::fs;

/// Full report from a single scan pass.
#[derive(Debug, Serialize)]
pub struct ScanReport {
    pub scan_time_secs: f64,
    pub total_markets: usize,
    pub total_orderbooks_fetched: usize,
    pub total_orderbooks_failed: usize,
    pub opportunities: Vec<OpportunityRow>,
    pub market_summary: Vec<MarketSummaryRow>,
}

/// Flat row for a single opportunity, suitable for CSV/table display.
#[derive(Debug, Clone, Serialize)]
pub struct OpportunityRow {
    pub rank: usize,
    pub arb_type: String,
    pub edge_bps: f64,
    pub vwap_edge_100: f64,
    pub vwap_edge_500: f64,
    pub vwap_edge_1000: f64,
    pub vwap_edge_5000: f64,
    pub num_markets: usize,
    pub question: String,
    /// Comma-separated condition IDs for all markets involved.
    pub condition_ids: String,
    pub confidence: f64,
}

/// Per-category summary of market health metrics.
#[derive(Debug, Clone, Serialize)]
pub struct MarketSummaryRow {
    pub category: String,
    pub market_count: usize,
    pub avg_spread_bps: f64,
    pub total_volume_24h: f64,
    pub avg_depth: f64,
}

/// Serialize a full `ScanReport` to pretty-printed JSON and write it to `path`.
pub fn export_json(report: &ScanReport, path: &str) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(report)
        .context("failed to serialize ScanReport to JSON")?;
    fs::write(path, json).with_context(|| format!("failed to write JSON to {path}"))?;
    Ok(())
}

/// Write a slice of `OpportunityRow` records as CSV to `path`.
pub fn export_csv(opportunities: &[OpportunityRow], path: &str) -> anyhow::Result<()> {
    let file =
        fs::File::create(path).with_context(|| format!("failed to create CSV file at {path}"))?;
    let buf = std::io::BufWriter::new(file);
    let mut wtr = csv::Writer::from_writer(buf);
    for row in opportunities {
        wtr.serialize(row)
            .context("failed to serialize OpportunityRow to CSV")?;
    }
    wtr.flush().context("failed to flush CSV writer")?;
    Ok(())
}

/// Convert a core [`Opportunity`] plus display metadata into a flat [`OpportunityRow`].
///
/// `vwap_edges_bps` contains the VWAP edge in basis points at the four standard
/// size tiers: \[100, 500, 1000, 5000\] USDC.
pub fn opportunity_to_row(
    rank: usize,
    opp: &Opportunity,
    question: &str,
    vwap_edges_bps: &[f64; 4],
) -> OpportunityRow {
    use rust_decimal::prelude::ToPrimitive;

    OpportunityRow {
        rank,
        arb_type: opp.arb_type.to_string(),
        edge_bps: opp.net_edge_bps().to_f64().unwrap_or(0.0),
        vwap_edge_100: vwap_edges_bps[0],
        vwap_edge_500: vwap_edges_bps[1],
        vwap_edge_1000: vwap_edges_bps[2],
        vwap_edge_5000: vwap_edges_bps[3],
        num_markets: opp.markets.len(),
        question: question.to_owned(),
        condition_ids: opp.markets.join(","),
        confidence: opp.confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{ArbType, Side, StrategyType, TradeLeg};
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    /// Build a minimal `Opportunity` for test purposes.
    fn sample_opportunity() -> Opportunity {
        Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            strategy_type: StrategyType::IntraMarketArb,
            markets: vec!["0xabc123".to_owned(), "0xdef456".to_owned()],
            legs: vec![TradeLeg {
                token_id: "tok1".to_owned(),
                side: Side::Buy,
                target_price: dec!(0.48),
                target_size: dec!(100),
                vwap_estimate: dec!(0.482),
            }],
            gross_edge: dec!(0.015),
            net_edge: dec!(0.012),
            estimated_vwap: vec![dec!(0.482)],
            confidence: 0.87,
            size_available: dec!(500),
            detected_at: Utc::now(),
        }
    }

    fn sample_rows() -> Vec<OpportunityRow> {
        vec![
            OpportunityRow {
                rank: 1,
                arb_type: "intra_market".to_owned(),
                edge_bps: 120.0,
                vwap_edge_100: 115.0,
                vwap_edge_500: 110.0,
                vwap_edge_1000: 100.0,
                vwap_edge_5000: 80.0,
                num_markets: 2,
                question: "Will it rain tomorrow?".to_owned(),
                condition_ids: "0xabc,0xdef".to_owned(),
                confidence: 0.87,
            },
            OpportunityRow {
                rank: 2,
                arb_type: "cross_market".to_owned(),
                edge_bps: 95.0,
                vwap_edge_100: 90.0,
                vwap_edge_500: 85.0,
                vwap_edge_1000: 75.0,
                vwap_edge_5000: 50.0,
                num_markets: 3,
                question: "Will candidate X win?".to_owned(),
                condition_ids: "0x111,0x222,0x333".to_owned(),
                confidence: 0.72,
            },
        ]
    }

    fn sample_report() -> ScanReport {
        ScanReport {
            scan_time_secs: 3.14,
            total_markets: 42,
            total_orderbooks_fetched: 40,
            total_orderbooks_failed: 2,
            opportunities: sample_rows(),
            market_summary: vec![MarketSummaryRow {
                category: "politics".to_owned(),
                market_count: 15,
                avg_spread_bps: 35.0,
                total_volume_24h: 150_000.0,
                avg_depth: 2500.0,
            }],
        }
    }

    #[test]
    fn test_json_export() {
        let report = sample_report();
        let path = "/tmp/arb_test_export.json";

        export_json(&report, path).expect("export_json should succeed");

        let contents = std::fs::read_to_string(path).expect("should read back JSON file");
        assert!(contents.contains("\"scan_time_secs\": 3.14"));
        assert!(contents.contains("intra_market"));
        assert!(contents.contains("cross_market"));
        assert!(contents.contains("politics"));
        assert!(contents.contains("Will it rain tomorrow?"));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_csv_export() {
        let rows = sample_rows();
        let path = "/tmp/arb_test_export.csv";

        export_csv(&rows, path).expect("export_csv should succeed");

        let contents = std::fs::read_to_string(path).expect("should read back CSV file");

        // Verify header row.
        assert!(contents.contains("rank"));
        assert!(contents.contains("arb_type"));
        assert!(contents.contains("edge_bps"));
        assert!(contents.contains("vwap_edge_100"));
        assert!(contents.contains("confidence"));

        // Verify data rows.
        assert!(contents.contains("intra_market"));
        assert!(contents.contains("cross_market"));
        assert!(contents.contains("Will it rain tomorrow?"));
        assert!(contents.contains("Will candidate X win?"));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_opportunity_to_row() {
        let opp = sample_opportunity();
        let vwap_edges = [115.0, 110.0, 100.0, 80.0];

        let row = opportunity_to_row(1, &opp, "Test question?", &vwap_edges);

        assert_eq!(row.rank, 1);
        assert_eq!(row.arb_type, "intra_market");
        // net_edge = 0.012 => net_edge_bps = 120
        assert!((row.edge_bps - 120.0).abs() < 0.01);
        assert!((row.vwap_edge_100 - 115.0).abs() < f64::EPSILON);
        assert!((row.vwap_edge_500 - 110.0).abs() < f64::EPSILON);
        assert!((row.vwap_edge_1000 - 100.0).abs() < f64::EPSILON);
        assert!((row.vwap_edge_5000 - 80.0).abs() < f64::EPSILON);
        assert_eq!(row.num_markets, 2);
        assert_eq!(row.question, "Test question?");
        assert_eq!(row.condition_ids, "0xabc123,0xdef456");
        assert!((row.confidence - 0.87).abs() < f64::EPSILON);
    }
}
