use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

const DEFAULT_TIERED_THRESHOLD: u64 = 200_000;
const MILLION: f64 = 1_000_000.0;
const DEFAULT_CODEX_FAST_MULTIPLIER: f64 = 2.0;

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
    pub provider_specific_entry: Option<ProviderSpecificEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSpecificEntry {
    pub fast: Option<f64>,
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
pub(crate) struct CacheCreationTokens {
    pub ephemeral_5m_input_tokens: u64,
    pub ephemeral_1h_input_tokens: u64,
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
            model_aliases: HashMap::from([
                ("gpt-5-codex".to_string(), "gpt-5".to_string()),
                ("gpt-5.3-codex".to_string(), "gpt-5".to_string()),
                ("claude-opus-4.5".to_string(), "claude-opus-4-5".to_string()),
                (
                    "claude-sonnet-4.5".to_string(),
                    "claude-sonnet-4-5".to_string(),
                ),
                (
                    "claude-haiku-4.5".to_string(),
                    "claude-haiku-4-5".to_string(),
                ),
                (
                    "claude-opus-4".to_string(),
                    "claude-opus-4-20250514".to_string(),
                ),
                ("claude-opus-41".to_string(), "claude-opus-4-1".to_string()),
                (
                    "claude-sonnet-4".to_string(),
                    "claude-sonnet-4-20250514".to_string(),
                ),
                (
                    "claude-3.5-sonnet".to_string(),
                    "claude-3-5-sonnet-latest".to_string(),
                ),
                (
                    "claude-3.7-sonnet".to_string(),
                    "claude-3-7-sonnet-latest".to_string(),
                ),
                (
                    "claude-3.7-sonnet-thought".to_string(),
                    "claude-3-7-sonnet-latest".to_string(),
                ),
                (
                    "grok-code-fast-1".to_string(),
                    "xai/grok-code-fast-1".to_string(),
                ),
                (
                    "gemini-3-pro-high".to_string(),
                    "gemini-3-pro-preview".to_string(),
                ),
                (
                    "gemini-3-pro".to_string(),
                    "gemini-3-pro-preview".to_string(),
                ),
                (
                    "kimi-k2.5".to_string(),
                    "openrouter/moonshotai/kimi-k2.5".to_string(),
                ),
                ("opus-4.5".to_string(), "claude-opus-4-5".to_string()),
                ("sonnet-4.5".to_string(), "claude-sonnet-4-5".to_string()),
                (
                    "opus-4-1-20250805".to_string(),
                    "claude-opus-4-1-20250805".to_string(),
                ),
                (
                    "opus-4-5-20251101".to_string(),
                    "claude-opus-4-5-20251101".to_string(),
                ),
                ("opus-4-6".to_string(), "claude-opus-4-6".to_string()),
                (
                    "sonnet-4-5-20250929".to_string(),
                    "claude-sonnet-4-5-20250929".to_string(),
                ),
                (
                    "opus-4-20250514".to_string(),
                    "claude-opus-4-20250514".to_string(),
                ),
                ("opus-4-5".to_string(), "claude-opus-4-5".to_string()),
                (
                    "haiku-4-5-20251001".to_string(),
                    "claude-haiku-4-5-20251001".to_string(),
                ),
                (
                    "sonnet-4-20250514".to_string(),
                    "claude-sonnet-4-20250514".to_string(),
                ),
                ("sonnet-4-6".to_string(), "claude-sonnet-4-6".to_string()),
                ("sonnet-4-5".to_string(), "claude-sonnet-4-5".to_string()),
            ]),
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
        self.calculate_cost_from_pricing_with_cache_creation(tokens, None, pricing)
    }

    fn calculate_cost_from_pricing_with_cache_creation(
        &self,
        tokens: &UsageTokens,
        cache_creation: Option<&CacheCreationTokens>,
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
        let (cache_creation_5m_tokens, cache_creation_1h_tokens) =
            if let Some(cache_creation) = cache_creation {
                (
                    cache_creation.ephemeral_5m_input_tokens,
                    cache_creation.ephemeral_1h_input_tokens,
                )
            } else {
                (tokens.cache_creation_input_tokens, 0)
            };
        let cache_creation_5m_cost = calculate_tiered_cost(
            cache_creation_5m_tokens,
            pricing.cache_creation_input_token_cost,
            pricing.cache_creation_input_token_cost_above_200k_tokens,
            DEFAULT_TIERED_THRESHOLD,
        );
        let cache_creation_1h_cost = calculate_tiered_cost(
            cache_creation_1h_tokens,
            pricing.input_cost_per_token.map(|cost| cost * 2.0),
            pricing
                .input_cost_per_token_above_200k_tokens
                .map(|cost| cost * 2.0),
            DEFAULT_TIERED_THRESHOLD,
        );
        let cache_read_cost = calculate_tiered_cost(
            tokens.cache_read_input_tokens,
            pricing.cache_read_input_token_cost,
            pricing.cache_read_input_token_cost_above_200k_tokens,
            DEFAULT_TIERED_THRESHOLD,
        );

        input_cost + output_cost + cache_creation_5m_cost + cache_creation_1h_cost + cache_read_cost
    }

    pub fn calculate_cost_from_tokens(
        &self,
        tokens: &UsageTokens,
        model_name: Option<&str>,
    ) -> f64 {
        self.calculate_cost_from_tokens_with_cache_creation(tokens, None, model_name)
    }

    pub(crate) fn calculate_cost_from_tokens_with_cache_creation(
        &self,
        tokens: &UsageTokens,
        cache_creation: Option<&CacheCreationTokens>,
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

        self.calculate_cost_from_pricing_with_cache_creation(tokens, cache_creation, &pricing)
    }

    pub fn calculate_codex_cost_from_tokens(
        &self,
        tokens: &UsageTokens,
        model_name: Option<&str>,
        fast_speed: bool,
    ) -> f64 {
        let model_name = match model_name {
            Some(name) if !name.is_empty() => name,
            _ => return 0.0,
        };

        let pricing = match self.get_model_pricing(model_name) {
            Some(pricing) => pricing,
            None => return 0.0,
        };

        let non_cached_input_tokens = tokens.input_tokens;
        let multiplier = if fast_speed {
            pricing
                .provider_specific_entry
                .as_ref()
                .and_then(|entry| entry.fast)
                .unwrap_or_else(|| codex_fast_multiplier_for_model(model_name))
        } else {
            1.0
        };

        let input_cost_per_million =
            pricing.input_cost_per_token.unwrap_or(0.0) * MILLION * multiplier;
        let cached_input_cost_per_million = pricing
            .cache_read_input_token_cost
            .or(pricing.input_cost_per_token)
            .unwrap_or(0.0)
            * MILLION
            * multiplier;
        let output_cost_per_million =
            pricing.output_cost_per_token.unwrap_or(0.0) * MILLION * multiplier;

        (non_cached_input_tokens as f64 / MILLION) * input_cost_per_million
            + (tokens.cache_read_input_tokens as f64 / MILLION) * cached_input_cost_per_million
            + (tokens.output_tokens as f64 / MILLION) * output_cost_per_million
    }
}

fn codex_fast_multiplier_for_model(model_name: &str) -> f64 {
    match model_name {
        "gpt-5.5" | "gpt-5.5-2026-04-23" => 2.5,
        _ => DEFAULT_CODEX_FAST_MULTIPLIER,
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
    fn calculate_cost_uses_separate_five_minute_and_one_hour_cache_rates() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_input_tokens: 999,
            cache_read_input_tokens: 0,
        };
        let cache_creation = CacheCreationTokens {
            ephemeral_5m_input_tokens: 10,
            ephemeral_1h_input_tokens: 20,
        };
        let pricing = LiteLLMModelPricing {
            input_cost_per_token: Some(1.0),
            output_cost_per_token: None,
            cache_creation_input_token_cost: Some(1.5),
            cache_read_input_token_cost: None,
            input_cost_per_token_above_200k_tokens: None,
            output_cost_per_token_above_200k_tokens: None,
            cache_creation_input_token_cost_above_200k_tokens: None,
            cache_read_input_token_cost_above_200k_tokens: None,
            max_input_tokens: None,
            provider_specific_entry: None,
        };

        let cost = fetcher.calculate_cost_from_pricing_with_cache_creation(
            &tokens,
            Some(&cache_creation),
            &pricing,
        );

        assert_eq!(cost, 55.0);
    }

    #[test]
    fn calculate_cost_keeps_flat_cache_creation_fallback() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_input_tokens: 10,
            cache_read_input_tokens: 0,
        };
        let pricing = LiteLLMModelPricing {
            input_cost_per_token: Some(1.0),
            output_cost_per_token: None,
            cache_creation_input_token_cost: Some(1.5),
            cache_read_input_token_cost: None,
            input_cost_per_token_above_200k_tokens: None,
            output_cost_per_token_above_200k_tokens: None,
            cache_creation_input_token_cost_above_200k_tokens: None,
            cache_read_input_token_cost_above_200k_tokens: None,
            max_input_tokens: None,
            provider_specific_entry: None,
        };

        assert_eq!(fetcher.calculate_cost_from_pricing(&tokens, &pricing), 15.0);
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

    #[test]
    fn calculate_codex_cost_applies_gpt_5_5_fast_multiplier() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 600_000,
            output_tokens: 10_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 400_000,
        };

        let standard = fetcher.calculate_codex_cost_from_tokens(&tokens, Some("gpt-5.5"), false);
        let fast = fetcher.calculate_codex_cost_from_tokens(&tokens, Some("gpt-5.5"), true);

        assert!((standard - 3.5).abs() < 1e-12);
        assert!((fast - standard * 2.5).abs() < 1e-12);
    }

    #[test]
    fn calculate_cost_from_tokens_supports_github_copilot_claude_alias() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let cost = fetcher.calculate_cost_from_tokens(&tokens, Some("claude-opus-4.5"));
        assert!(cost > 0.0);
    }

    #[test]
    fn calculate_cost_from_tokens_supports_opencode_short_claude_names() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 50,
            cache_read_input_tokens: 100,
        };
        let cost = fetcher.calculate_cost_from_tokens(&tokens, Some("opus-4-6"));
        assert!(cost > 0.0);
    }

    #[test]
    fn calculate_cost_from_tokens_supports_kimi_and_gemini_aliases() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 100,
        };
        let gemini_cost = fetcher.calculate_cost_from_tokens(&tokens, Some("gemini-3-pro"));
        let kimi_cost = fetcher.calculate_cost_from_tokens(&tokens, Some("kimi-k2.5"));
        assert!(gemini_cost > 0.0);
        assert!(kimi_cost > 0.0);
    }
}
