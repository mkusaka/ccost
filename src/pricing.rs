use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

const DEFAULT_TIERED_THRESHOLD: u64 = 200_000;
const MILLION: f64 = 1_000_000.0;
const DEFAULT_CODEX_FAST_MULTIPLIER: f64 = 2.0;
const DEEPSEEK_V4_PRICING_CUTOFF_MS: i64 = 1_786_896_000_000;
const MILLIS_PER_HOUR: i64 = 3_600_000;
const MILLIS_PER_DAY: i64 = 86_400_000;

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
    devin_pricing: bool,
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
                ("GPT-5.6 Sol".to_string(), "gpt-5.6-sol".to_string()),
                ("Claude Fable 5".to_string(), "claude-fable-5".to_string()),
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
                ("SWE-1.7".to_string(), "cognition/swe-1.7".to_string()),
                ("SWE-1.7 Max".to_string(), "cognition/swe-1.7".to_string()),
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
            devin_pricing: false,
        }
    }

    /// Fetcher for Devin transcripts: some models are billed by Devin at rates
    /// that differ from the provider list prices used by other agents.
    pub(crate) fn new_devin() -> Self {
        Self {
            devin_pricing: true,
            ..Self::new()
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

        // Agents like Devin report display names ("DeepSeek V4.1 Flash Max") or
        // slugs ("deepseek-v4-1-flash-max") that carry a reasoning-effort
        // suffix. Compare a normalized, suffix-stripped form against the last
        // segment of each pricing key, preferring unprefixed (direct-provider)
        // entries over gateway variants.
        let slug = normalize_model_key(model_name);
        for form in [&slug, strip_effort_suffix(&slug).unwrap_or(&slug)] {
            if self.devin_pricing
                && let Some(pricing) = devin_model_pricing(form)
            {
                return Some(pricing);
            }
            let mut prefixed_match = None;
            for (key, value) in pricing {
                if key.contains(':') {
                    continue;
                }
                let candidate = normalize_model_key(key.rsplit('/').next().unwrap_or(key));
                if candidate == *form {
                    if !key.contains('/') {
                        return Some(value.clone());
                    }
                    prefixed_match = prefixed_match.or(Some(value));
                }
            }
            if let Some(value) = prefixed_match {
                return Some(value.clone());
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

        self.calculate_codex_cost(tokens, model_name, &pricing, fast_speed)
    }

    pub(crate) fn has_time_dependent_pricing(&self, model_name: &str) -> bool {
        deepseek_v4_model_identity(model_name).is_some()
            || self
                .model_aliases
                .get(model_name)
                .and_then(|alias| deepseek_v4_model_identity(alias))
                .is_some()
    }

    pub(crate) fn calculate_codex_cost_at(
        &self,
        tokens: &UsageTokens,
        model_name: &str,
        timestamp_ms: i64,
        fast_speed: bool,
    ) -> f64 {
        let pricing = match self.get_model_pricing_at(model_name, timestamp_ms) {
            Some(pricing) => pricing,
            None => return 0.0,
        };
        self.calculate_codex_cost(tokens, model_name, &pricing, fast_speed)
    }

    fn get_model_pricing_at(
        &self,
        model_name: &str,
        timestamp_ms: i64,
    ) -> Option<LiteLLMModelPricing> {
        let identity = deepseek_v4_model_identity(model_name).or_else(|| {
            self.model_aliases
                .get(model_name)
                .and_then(|alias| deepseek_v4_model_identity(alias))
        });
        match identity {
            Some(identity) => Some(deepseek_v4_scheduled_pricing(identity, timestamp_ms)),
            None => self.get_model_pricing(model_name),
        }
    }

    fn calculate_codex_cost(
        &self,
        tokens: &UsageTokens,
        model_name: &str,
        pricing: &LiteLLMModelPricing,
        fast_speed: bool,
    ) -> f64 {
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
        let cache_creation_cost_per_million = pricing
            .cache_creation_input_token_cost
            .unwrap_or_else(|| pricing.input_cost_per_token.unwrap_or(0.0) * 1.25)
            * MILLION
            * multiplier;
        let output_cost_per_million =
            pricing.output_cost_per_token.unwrap_or(0.0) * MILLION * multiplier;

        (non_cached_input_tokens as f64 / MILLION) * input_cost_per_million
            + (tokens.cache_read_input_tokens as f64 / MILLION) * cached_input_cost_per_million
            + (tokens.cache_creation_input_tokens as f64 / MILLION)
                * cache_creation_cost_per_million
            + (tokens.output_tokens as f64 / MILLION) * output_cost_per_million
    }
}

fn normalize_model_key(name: &str) -> String {
    name.to_lowercase().replace([' ', '.', '_'], "-")
}

fn devin_model_pricing(slug: &str) -> Option<LiteLLMModelPricing> {
    // Rates shown by `devin models list`, per token: input, cached input, output.
    // Devin-only models without per-token pricing (e.g. SWE-2 is listed as
    // Free) are intentionally absent.
    let (input, cached, output) = match slug {
        "deepseek-v4-1-flash" => (0.22e-6, 0.01e-6, 0.66e-6),
        "deepseek-v4-flash" => (0.14e-6, 0.03e-6, 0.28e-6),
        "adaptive" => (0.5e-6, 0.1e-6, 2e-6),
        _ => return None,
    };
    Some(LiteLLMModelPricing {
        input_cost_per_token: Some(input),
        output_cost_per_token: Some(output),
        cache_creation_input_token_cost: Some(input),
        cache_read_input_token_cost: Some(cached),
        input_cost_per_token_above_200k_tokens: Some(input),
        output_cost_per_token_above_200k_tokens: Some(output),
        cache_creation_input_token_cost_above_200k_tokens: Some(input),
        cache_read_input_token_cost_above_200k_tokens: Some(cached),
        max_input_tokens: None,
        provider_specific_entry: None,
    })
}

fn strip_effort_suffix(slug: &str) -> Option<&str> {
    for suffix in ["-minimal", "-xhigh", "-medium", "-high", "-low", "-max"] {
        if let Some(stripped) = slug.strip_suffix(suffix)
            && !stripped.is_empty()
        {
            return Some(stripped);
        }
    }
    None
}

fn codex_fast_multiplier_for_model(model_name: &str) -> f64 {
    match model_name {
        "gpt-5.5" | "gpt-5.5-2026-04-23" => 2.5,
        _ => DEFAULT_CODEX_FAST_MULTIPLIER,
    }
}

#[derive(Clone, Copy)]
struct DeepSeekV4Rates {
    input: f64,
    output: f64,
    cache_create: f64,
    cache_read: f64,
}

fn deepseek_v4_model_identity(model: &str) -> Option<&'static str> {
    match model {
        "deepseek-v4-flash" => Some("deepseek-v4-flash"),
        "deepseek-v4-pro" => Some("deepseek-v4-pro"),
        _ => None,
    }
}

fn deepseek_v4_rates(model: &str, timestamp_ms: i64) -> Option<DeepSeekV4Rates> {
    let (old, off_peak, peak) = match model {
        "deepseek-v4-flash" => (
            DeepSeekV4Rates {
                input: 0.14e-6,
                output: 0.28e-6,
                cache_create: 0.14e-6,
                cache_read: 0.0028e-6,
            },
            DeepSeekV4Rates {
                input: 0.22e-6,
                output: 0.66e-6,
                cache_create: 0.22e-6,
                cache_read: 0.007e-6,
            },
            DeepSeekV4Rates {
                input: 0.44e-6,
                output: 1.32e-6,
                cache_create: 0.44e-6,
                cache_read: 0.014e-6,
            },
        ),
        "deepseek-v4-pro" => (
            DeepSeekV4Rates {
                input: 0.435e-6,
                output: 0.87e-6,
                cache_create: 0.435e-6,
                cache_read: 0.003625e-6,
            },
            DeepSeekV4Rates {
                input: 0.66e-6,
                output: 1.98e-6,
                cache_create: 0.66e-6,
                cache_read: 0.022e-6,
            },
            DeepSeekV4Rates {
                input: 1.32e-6,
                output: 3.96e-6,
                cache_create: 1.32e-6,
                cache_read: 0.044e-6,
            },
        ),
        _ => return None,
    };
    if timestamp_ms < DEEPSEEK_V4_PRICING_CUTOFF_MS {
        return Some(old);
    }
    Some(if deepseek_v4_peak(timestamp_ms) {
        peak
    } else {
        off_peak
    })
}

fn deepseek_v4_peak(timestamp_ms: i64) -> bool {
    // DeepSeek publishes these windows in UTC, so use the epoch instant rather
    // than the report's display timezone when deriving the calendar buckets.
    let days_since_epoch = timestamp_ms.div_euclid(MILLIS_PER_DAY);
    // Unix epoch Thursday is weekday 4 when Sunday is zero; Euclidean modulo
    // keeps the mapping valid for timestamps before the epoch as well.
    let weekday_from_sunday = (days_since_epoch + 4).rem_euclid(7);
    // Saturdays and Sundays are always off-peak.
    if !(1..=5).contains(&weekday_from_sunday) {
        return false;
    }
    // The published windows are half-open, so their ending hours are excluded.
    let hour = timestamp_ms.rem_euclid(MILLIS_PER_DAY) / MILLIS_PER_HOUR;
    (1..4).contains(&hour) || (6..10).contains(&hour)
}

fn deepseek_v4_scheduled_pricing(identity: &str, timestamp_ms: i64) -> LiteLLMModelPricing {
    let rates = deepseek_v4_rates(identity, timestamp_ms).expect("scheduled model");
    LiteLLMModelPricing {
        input_cost_per_token: Some(rates.input),
        output_cost_per_token: Some(rates.output),
        cache_creation_input_token_cost: Some(rates.cache_create),
        cache_read_input_token_cost: Some(rates.cache_read),
        input_cost_per_token_above_200k_tokens: Some(rates.input),
        output_cost_per_token_above_200k_tokens: Some(rates.output),
        cache_creation_input_token_cost_above_200k_tokens: Some(rates.cache_create),
        cache_read_input_token_cost_above_200k_tokens: Some(rates.cache_read),
        max_input_tokens: None,
        provider_specific_entry: Some(ProviderSpecificEntry { fast: Some(1.0) }),
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
    fn resolves_agent_display_names_and_effort_suffixes() {
        let fetcher = PricingFetcher::new();
        let cases = [
            ("DeepSeek V4.1 Flash Max", 3e-7),
            ("DeepSeek V4.1 Flash High", 3e-7),
            ("deepseek-v4-1-flash-max", 3e-7),
            ("GLM-5.2 High", 1.4e-6),
            ("glm-5-2", 1.4e-6),
            ("gpt-6-astra-medium", 1e-5),
            ("SWE-1.7 Max", 5e-7),
            ("swe-1-7", 5e-7),
        ];
        for (name, input_cost) in cases {
            let pricing = fetcher
                .get_model_pricing(name)
                .unwrap_or_else(|| panic!("no pricing for {name}"));
            assert_eq!(pricing.input_cost_per_token, Some(input_cost), "{name}");
        }
        // No public per-token pricing exists for SWE-2 yet.
        assert!(fetcher.get_model_pricing("SWE-2 Max").is_none());
    }

    #[test]
    fn devin_fetcher_uses_devin_rates() {
        let fetcher = PricingFetcher::new_devin();
        let cases = [
            ("DeepSeek V4.1 Flash Max", 0.22e-6),
            ("deepseek-v4-1-flash-high", 0.22e-6),
            ("DeepSeek V4 Flash Max", 0.14e-6),
            ("Adaptive", 0.5e-6),
        ];
        for (name, input_cost) in cases {
            let pricing = fetcher
                .get_model_pricing(name)
                .unwrap_or_else(|| panic!("no pricing for {name}"));
            assert_eq!(pricing.input_cost_per_token, Some(input_cost), "{name}");
        }
        // SWE-2 is listed as Free by `devin models list`.
        assert!(fetcher.get_model_pricing("SWE-2 Max").is_none());
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
    fn codex_model_pricing_does_not_fall_back_to_gpt_5() {
        let fetcher = PricingFetcher::new();
        let gpt_5 = fetcher
            .get_model_pricing("gpt-5")
            .unwrap()
            .input_cost_per_token;
        let gpt_5_codex = fetcher
            .get_model_pricing("gpt-5.3-codex")
            .unwrap()
            .input_cost_per_token;

        assert_ne!(gpt_5, gpt_5_codex);
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
    fn calculate_cost_from_tokens_supports_devin_model_labels() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1_000,
            output_tokens: 100,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 100,
        };

        assert!(fetcher.calculate_cost_from_tokens(&tokens, Some("Claude Fable 5")) > 0.0);
        assert!(fetcher.calculate_cost_from_tokens(&tokens, Some("GPT-5.6 Sol")) > 0.0);
    }

    #[test]
    fn swe_models_resolve_to_cognition_pricing() {
        let fetcher = PricingFetcher::new();
        let tokens = UsageTokens {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };

        // SWE-2 has no published token rate; stays $0 until upstream lists it.
        let swe17 = fetcher.calculate_cost_from_tokens(&tokens, Some("SWE-1.7 Max"));
        let swe2 = fetcher.calculate_cost_from_tokens(&tokens, Some("SWE-2 High"));

        assert!((swe17 - 0.5).abs() < 1e-9);
        assert_eq!(swe2, 0.0);
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
