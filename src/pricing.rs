use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

const DEFAULT_TIERED_THRESHOLD: u64 = 200_000;

#[derive(Debug, Clone, Deserialize)]
pub struct LiteLLMModelPricing {
    pub input_cost_per_token: Option<f64>,
    pub output_cost_per_token: Option<f64>,
    pub cache_creation_input_token_cost: Option<f64>,
    pub cache_read_input_token_cost: Option<f64>,
    pub input_cost_per_token_above_200k_tokens: Option<f64>,
    pub output_cost_per_token_above_200k_tokens: Option<f64>,
    pub cache_creation_input_token_cost_above_200k_tokens: Option<f64>,
    pub cache_read_input_token_cost_above_200k_tokens: Option<f64>,
    pub max_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostMode {
    Auto,
    Calculate,
    Display,
}

impl FromStr for CostMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "calculate" => Ok(Self::Calculate),
            "display" => Ok(Self::Display),
            _ => Err(format!("Invalid cost mode: {value}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UsageTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

fn pricing_dataset() -> &'static HashMap<String, LiteLLMModelPricing> {
    static DATASET: OnceLock<HashMap<String, LiteLLMModelPricing>> = OnceLock::new();
    DATASET.get_or_init(|| {
        let claude = include_str!("../assets/claude_pricing.json");
        let codex = include_str!("../assets/codex_pricing.json");
        let mut merged: HashMap<String, LiteLLMModelPricing> =
            serde_json::from_str(claude).unwrap_or_default();
        let codex_entries: HashMap<String, LiteLLMModelPricing> =
            serde_json::from_str(codex).unwrap_or_default();
        merged.extend(codex_entries);
        merged
    })
}

pub struct PricingFetcher {
    provider_prefixes: Vec<String>,
    model_aliases: HashMap<String, String>,
}

impl Default for PricingFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl PricingFetcher {
    pub fn new() -> Self {
        Self {
            provider_prefixes: vec![
                "anthropic/".to_string(),
                "claude-3-5-".to_string(),
                "claude-3-".to_string(),
                "claude-".to_string(),
                "openai/".to_string(),
                "azure/".to_string(),
                "openrouter/openai/".to_string(),
            ],
            model_aliases: HashMap::from([("gpt-5-codex".to_string(), "gpt-5".to_string())]),
        }
    }

    fn candidate_names(&self, model_name: &str) -> Vec<String> {
        let mut candidates = Vec::with_capacity(self.provider_prefixes.len() + 1);
        candidates.push(model_name.to_string());
        for prefix in &self.provider_prefixes {
            candidates.push(format!("{prefix}{model_name}"));
        }
        candidates
    }

    pub fn get_model_pricing(&self, model_name: &str) -> Option<LiteLLMModelPricing> {
        let pricing = pricing_dataset();
        let mut names = vec![model_name.to_string()];
        if let Some(alias) = self.model_aliases.get(model_name) {
            names.push(alias.clone());
        }

        for name in names {
            for candidate in self.candidate_names(&name) {
                if let Some(found) = pricing.get(&candidate) {
                    return Some(found.clone());
                }
            }
        }

        let lower = model_name.to_lowercase();
        for (key, value) in pricing {
            let comparison = key.to_lowercase();
            if comparison.contains(&lower) || lower.contains(&comparison) {
                return Some(value.clone());
            }
        }

        None
    }

    pub fn calculate_cost_from_pricing(
        &self,
        tokens: &UsageTokens,
        pricing: &LiteLLMModelPricing,
    ) -> f64 {
        let calculate_tiered_cost =
            |total: u64, base: Option<f64>, tiered: Option<f64>, threshold: u64| -> f64 {
                if total == 0 {
                    return 0.0;
                }
                if total > threshold && tiered.is_some() {
                    let below = total.min(threshold) as f64;
                    let above = (total - threshold) as f64;
                    let mut cost = above * tiered.unwrap_or(0.0);
                    if let Some(base) = base {
                        cost += below * base;
                    }
                    return cost;
                }
                base.unwrap_or(0.0) * total as f64
            };

        let input_cost = calculate_tiered_cost(
            tokens.input_tokens,
            pricing.input_cost_per_token,
            pricing.input_cost_per_token_above_200k_tokens,
            DEFAULT_TIERED_THRESHOLD,
        );
        let output_cost = calculate_tiered_cost(
            tokens.output_tokens,
            pricing.output_cost_per_token,
            pricing.output_cost_per_token_above_200k_tokens,
            DEFAULT_TIERED_THRESHOLD,
        );
        let cache_creation_cost = calculate_tiered_cost(
            tokens.cache_creation_input_tokens,
            pricing.cache_creation_input_token_cost,
            pricing.cache_creation_input_token_cost_above_200k_tokens,
            DEFAULT_TIERED_THRESHOLD,
        );
        let cache_read_cost = calculate_tiered_cost(
            tokens.cache_read_input_tokens,
            pricing.cache_read_input_token_cost,
            pricing.cache_read_input_token_cost_above_200k_tokens,
            DEFAULT_TIERED_THRESHOLD,
        );

        input_cost + output_cost + cache_creation_cost + cache_read_cost
    }

    pub fn calculate_cost_from_tokens(
        &self,
        tokens: &UsageTokens,
        model_name: Option<&str>,
    ) -> f64 {
        let model_name = match model_name {
            Some(name) if !name.is_empty() => name,
            _ => return 0.0,
        };

        let pricing = match self.get_model_pricing(model_name) {
            Some(pricing) => pricing,
            None => return 0.0,
        };

        self.calculate_cost_from_pricing(tokens, &pricing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_cost_from_tokens_returns_zero_without_model() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        assert_eq!(fetcher.calculate_cost_from_tokens(&tokens, None), 0.0);
    }

    #[test]
    fn calculate_cost_from_tokens_uses_pricing() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 200,
            cache_read_input_tokens: 100,
        };
        let cost = fetcher.calculate_cost_from_tokens(&tokens, Some("claude-sonnet-4-20250514"));
        assert!(cost > 0.0);
    }

    #[test]
    fn calculate_cost_from_tokens_supports_codex_models() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 100,
        };
        let cost = fetcher.calculate_cost_from_tokens(&tokens, Some("gpt-5-codex"));
        assert!(cost > 0.0);
    }
}
