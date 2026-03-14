use std::collections::HashMap;
use std::path::Path;

use arb_core::{
    CorrelationRelationship, MarketCorrelation, MarketState,
    error::{ArbError, Result},
};
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::debug;

/// Stores user-defined logical relationships between markets.
///
/// Loaded from a TOML file with entries like:
/// ```toml
/// [[pairs]]
/// condition_id_a = "0xabc..."
/// condition_id_b = "0xdef..."
/// relationship = "implied_by"
/// ```
pub struct CorrelationGraph {
    pairs: Vec<MarketCorrelation>,
}

#[derive(Deserialize)]
struct CorrelationFile {
    #[serde(default)]
    pairs: Vec<CorrelationEntry>,
}

#[derive(Deserialize)]
struct CorrelationEntry {
    condition_id_a: String,
    condition_id_b: String,
    relationship: String,
    #[serde(default)]
    constraint: Option<String>,
    #[serde(default)]
    bound: Option<Decimal>,
}

impl CorrelationGraph {
    /// Load correlation pairs from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ArbError::Config(format!(
                "Cannot read correlation file {}: {e}",
                path.display()
            ))
        })?;

        let file: CorrelationFile = toml::from_str(&content)?;

        let pairs = file
            .pairs
            .into_iter()
            .map(|entry| {
                let relationship = match entry.relationship.as_str() {
                    "implied_by" => CorrelationRelationship::ImpliedBy,
                    "mutually_exclusive" => CorrelationRelationship::MutuallyExclusive,
                    "exhaustive" => CorrelationRelationship::Exhaustive,
                    "custom" => CorrelationRelationship::Custom {
                        constraint: entry.constraint.unwrap_or_default(),
                        bound: entry.bound.unwrap_or(Decimal::ZERO),
                    },
                    other => {
                        return Err(ArbError::Config(format!(
                            "Unknown relationship type: '{other}'"
                        )));
                    }
                };

                Ok(MarketCorrelation {
                    condition_id_a: entry.condition_id_a,
                    condition_id_b: entry.condition_id_b,
                    relationship,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { pairs })
    }

    /// Create an empty correlation graph.
    pub fn empty() -> Self {
        Self { pairs: vec![] }
    }

    pub fn pairs(&self) -> &[MarketCorrelation] {
        &self.pairs
    }

    /// Get all correlation pairs involving a specific market.
    pub fn pairs_for_market(&self, condition_id: &str) -> Vec<&MarketCorrelation> {
        self.pairs
            .iter()
            .filter(|p| p.condition_id_a == condition_id || p.condition_id_b == condition_id)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Merge auto-detected correlations into this graph.
    /// Skips pairs that already exist (by condition_id pair).
    pub fn merge(&mut self, new_pairs: Vec<MarketCorrelation>) {
        for pair in new_pairs {
            let already_exists = self.pairs.iter().any(|p| {
                (p.condition_id_a == pair.condition_id_a && p.condition_id_b == pair.condition_id_b)
                    || (p.condition_id_a == pair.condition_id_b
                        && p.condition_id_b == pair.condition_id_a)
            });
            if !already_exists {
                self.pairs.push(pair);
            }
        }
    }

    /// Auto-detect correlations from markets sharing the same `event_id`.
    ///
    /// Markets within the same event are assumed to be mutually exclusive
    /// (each represents a different outcome of the same question).
    /// Generates `MutuallyExclusive` pairs for all combinations.
    pub fn auto_detect(markets: &[MarketState]) -> Vec<MarketCorrelation> {
        let mut by_event: HashMap<&str, Vec<&MarketState>> = HashMap::new();

        for market in markets {
            if !market.active {
                continue;
            }
            if let Some(ref eid) = market.event_id {
                by_event.entry(eid.as_str()).or_default().push(market);
            }
        }

        let mut auto_pairs = Vec::new();

        for (event_id, group) in &by_event {
            if group.len() < 2 {
                continue;
            }

            // For events with 2+ markets: all pairs are mutually exclusive
            for i in 0..group.len() {
                for j in (i + 1)..group.len() {
                    auto_pairs.push(MarketCorrelation {
                        condition_id_a: group[i].condition_id.clone(),
                        condition_id_b: group[j].condition_id.clone(),
                        relationship: CorrelationRelationship::MutuallyExclusive,
                    });
                }
            }

            debug!(
                event_id = %event_id,
                n_markets = group.len(),
                n_pairs = group.len() * (group.len() - 1) / 2,
                "Auto-detected mutually exclusive pairs"
            );
        }

        auto_pairs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_correlation_toml() {
        let toml_str = r#"
[[pairs]]
condition_id_a = "0xabc"
condition_id_b = "0xdef"
relationship = "implied_by"

[[pairs]]
condition_id_a = "0x123"
condition_id_b = "0x456"
relationship = "mutually_exclusive"
"#;
        let file: CorrelationFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.pairs.len(), 2);
        assert_eq!(file.pairs[0].relationship, "implied_by");
        assert_eq!(file.pairs[1].relationship, "mutually_exclusive");
    }

    #[test]
    fn test_empty_graph() {
        let graph = CorrelationGraph::empty();
        assert!(graph.is_empty());
        assert_eq!(graph.pairs_for_market("anything").len(), 0);
    }
}
