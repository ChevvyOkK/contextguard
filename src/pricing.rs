use std::collections::HashMap;

/// Per-million-token USD pricing. These are reasonable defaults based on
/// publicly published Anthropic API rates, NOT a live price feed — rates
/// change, and subscription-plan usage isn't billed per-token the same way
/// API usage is. Treat cost figures from this tool as an estimate for
/// spotting trends and waste, not an invoice. `--pricing <file>` overrides
/// these with your own numbers.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

const DEFAULT_SONNET: ModelPricing = ModelPricing {
    input_per_mtok: 3.0,
    output_per_mtok: 15.0,
    cache_write_per_mtok: 3.75,
    cache_read_per_mtok: 0.30,
};

const DEFAULT_OPUS: ModelPricing = ModelPricing {
    input_per_mtok: 15.0,
    output_per_mtok: 75.0,
    cache_write_per_mtok: 18.75,
    cache_read_per_mtok: 1.50,
};

const DEFAULT_HAIKU: ModelPricing = ModelPricing {
    input_per_mtok: 0.80,
    output_per_mtok: 4.0,
    cache_write_per_mtok: 1.0,
    cache_read_per_mtok: 0.08,
};

pub struct PricingTable {
    overrides: HashMap<String, ModelPricing>,
}

impl PricingTable {
    pub fn defaults() -> Self {
        PricingTable { overrides: HashMap::new() }
    }

    pub fn for_model(&self, model: &str) -> ModelPricing {
        if let Some(p) = self.overrides.get(model) {
            return *p;
        }
        let lower = model.to_ascii_lowercase();
        if lower.contains("opus") {
            DEFAULT_OPUS
        } else if lower.contains("haiku") {
            DEFAULT_HAIKU
        } else {
            // Sonnet is the default fallback since it's the most common
            // day-to-day model; unrecognized names still get an estimate
            // rather than silently reporting $0.
            DEFAULT_SONNET
        }
    }

    pub fn cost_usd(&self, model: &str, usage: &crate::session::Usage) -> f64 {
        let p = self.for_model(model);
        let mtok = 1_000_000.0;
        (usage.input_tokens as f64 / mtok) * p.input_per_mtok
            + (usage.output_tokens as f64 / mtok) * p.output_per_mtok
            + (usage.cache_creation_input_tokens as f64 / mtok) * p.cache_write_per_mtok
            + (usage.cache_read_input_tokens as f64 / mtok) * p.cache_read_per_mtok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Usage;

    #[test]
    fn sonnet_cost_matches_hand_calculation() {
        let table = PricingTable::defaults();
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let cost = table.cost_usd("claude-sonnet-5", &usage);
        assert!((cost - 18.0).abs() < 0.001, "got {cost}");
    }

    #[test]
    fn cache_read_is_much_cheaper_than_fresh_input() {
        let table = PricingTable::defaults();
        let fresh = Usage {
            input_tokens: 100_000,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let cached = Usage {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 100_000,
        };
        let table_ = &table;
        assert!(table_.cost_usd("claude-sonnet-5", &cached) < table_.cost_usd("claude-sonnet-5", &fresh));
    }

    #[test]
    fn unknown_model_falls_back_to_sonnet_not_zero() {
        let table = PricingTable::defaults();
        let usage = Usage {
            input_tokens: 1000,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        assert!(table.cost_usd("some-future-model-name", &usage) > 0.0);
    }

    #[test]
    fn opus_costs_more_than_sonnet_for_same_usage() {
        let table = PricingTable::defaults();
        let usage = Usage {
            input_tokens: 1000,
            output_tokens: 1000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        assert!(table.cost_usd("claude-opus-5", &usage) > table.cost_usd("claude-sonnet-5", &usage));
    }
}
