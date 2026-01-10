use crate::token_utils::{AggregatedTokenCounts, get_total_tokens_from_aggregated};
use num_format::{Locale, ToFormattedString};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct UsageDataRow {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_cost: f64,
    pub models_used: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ModelBreakdownRow {
    pub model_name: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum TableMode {
    Full,
    Compact,
}

pub fn format_number(num: f64) -> String {
    if num.is_nan() || num.is_infinite() {
        return num.to_string();
    }
    let raw = num.to_string();
    let (sign, rest) = raw
        .strip_prefix('-')
        .map_or(("", raw.as_str()), |v| ("-", v));
    let (int_part, frac_part) = rest.split_once('.').unwrap_or((rest, ""));
    let int_value: i128 = int_part.parse().unwrap_or(0);
    let grouped = int_value.to_formatted_string(&Locale::en);
    if frac_part.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac_part}")
    }
}

pub fn format_currency(amount: f64) -> String {
    format!("${amount:.2}")
}

fn format_model_name(model_name: &str) -> String {
    if let Some(caps) = Regex::new(r"^\[pi\] (.+)$")
        .ok()
        .and_then(|re| re.captures(model_name))
    {
        return format!("[pi] {}", format_model_name(&caps[1]));
    }

    if let Some(caps) = Regex::new(r"^anthropic/claude-(\w+)-([\d.]+)$")
        .ok()
        .and_then(|re| re.captures(model_name))
    {
        return format!("{}-{}", &caps[1], &caps[2]);
    }

    if let Some(caps) = Regex::new(r"^claude-(\w+)-([\d-]+)-(\d{8})$")
        .ok()
        .and_then(|re| re.captures(model_name))
    {
        return format!("{}-{}", &caps[1], &caps[2]);
    }

    if let Some(caps) = Regex::new(r"^claude-(\w+)-([\d-]+)$")
        .ok()
        .and_then(|re| re.captures(model_name))
    {
        return format!("{}-{}", &caps[1], &caps[2]);
    }

    model_name.to_string()
}

pub fn format_models_display(models: &[String]) -> String {
    let mut unique = models
        .iter()
        .map(|m| format_model_name(m))
        .collect::<Vec<_>>();
    unique.sort();
    unique.dedup();
    unique.join(", ")
}

pub fn format_models_display_multiline(models: &[String]) -> String {
    let mut unique = models
        .iter()
        .map(|m| format_model_name(m))
        .collect::<Vec<_>>();
    unique.sort();
    unique.dedup();
    unique
        .into_iter()
        .map(|model| format!("- {model}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_usage_row(
    first_column_value: &str,
    data: &UsageDataRow,
    mode: TableMode,
) -> Vec<String> {
    let totals = AggregatedTokenCounts {
        input_tokens: data.input_tokens,
        output_tokens: data.output_tokens,
        cache_creation_tokens: data.cache_creation_tokens,
        cache_read_tokens: data.cache_read_tokens,
    };
    let total_tokens = get_total_tokens_from_aggregated(totals);

    match mode {
        TableMode::Full => vec![
            first_column_value.to_string(),
            format_models_display_multiline(&data.models_used),
            format_number(data.input_tokens as f64),
            format_number(data.output_tokens as f64),
            format_number(data.cache_creation_tokens as f64),
            format_number(data.cache_read_tokens as f64),
            format_number(total_tokens as f64),
            format_currency(data.total_cost),
        ],
        TableMode::Compact => vec![
            first_column_value.to_string(),
            format_models_display_multiline(&data.models_used),
            format_number(data.input_tokens as f64),
            format_number(data.output_tokens as f64),
            format_currency(data.total_cost),
        ],
    }
}

pub fn build_totals_row(totals: &UsageDataRow, mode: TableMode) -> Vec<String> {
    let totals_counts = AggregatedTokenCounts {
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cache_creation_tokens: totals.cache_creation_tokens,
        cache_read_tokens: totals.cache_read_tokens,
    };
    let total_tokens = get_total_tokens_from_aggregated(totals_counts);

    match mode {
        TableMode::Full => vec![
            "Total".to_string(),
            String::new(),
            format_number(totals.input_tokens as f64),
            format_number(totals.output_tokens as f64),
            format_number(totals.cache_creation_tokens as f64),
            format_number(totals.cache_read_tokens as f64),
            format_number(total_tokens as f64),
            format_currency(totals.total_cost),
        ],
        TableMode::Compact => vec![
            "Total".to_string(),
            String::new(),
            format_number(totals.input_tokens as f64),
            format_number(totals.output_tokens as f64),
            format_currency(totals.total_cost),
        ],
    }
}

pub fn build_breakdown_rows(breakdowns: &[ModelBreakdownRow], mode: TableMode) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for breakdown in breakdowns {
        let totals = AggregatedTokenCounts {
            input_tokens: breakdown.input_tokens,
            output_tokens: breakdown.output_tokens,
            cache_creation_tokens: breakdown.cache_creation_tokens,
            cache_read_tokens: breakdown.cache_read_tokens,
        };
        let total_tokens = get_total_tokens_from_aggregated(totals);
        match mode {
            TableMode::Full => rows.push(vec![
                format!("  |- {}", format_model_name(&breakdown.model_name)),
                String::new(),
                format_number(breakdown.input_tokens as f64),
                format_number(breakdown.output_tokens as f64),
                format_number(breakdown.cache_creation_tokens as f64),
                format_number(breakdown.cache_read_tokens as f64),
                format_number(total_tokens as f64),
                format_currency(breakdown.cost),
            ]),
            TableMode::Compact => rows.push(vec![
                format!("  |- {}", format_model_name(&breakdown.model_name)),
                String::new(),
                format_number(breakdown.input_tokens as f64),
                format_number(breakdown.output_tokens as f64),
                format_currency(breakdown.cost),
            ]),
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_formats_integers_with_commas() {
        assert_eq!(format_number(1000.0), "1,000");
        assert_eq!(format_number(1000000.0), "1,000,000");
        assert_eq!(format_number(1234567.89), "1,234,567.89");
    }

    #[test]
    fn format_number_handles_small_values() {
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(999.0), "999");
    }

    #[test]
    fn format_number_handles_negative_values() {
        assert_eq!(format_number(-1000.0), "-1,000");
        assert_eq!(format_number(-1234567.89), "-1,234,567.89");
    }

    #[test]
    fn format_number_handles_decimals() {
        assert_eq!(format_number(1234.56), "1,234.56");
        assert_eq!(format_number(0.123), "0.123");
    }

    #[test]
    fn format_currency_formats_amounts() {
        assert_eq!(format_currency(10.0), "$10.00");
        assert_eq!(format_currency(100.5), "$100.50");
        assert_eq!(format_currency(1234.56), "$1234.56");
    }

    #[test]
    fn format_currency_handles_zero_and_negative() {
        assert_eq!(format_currency(0.0), "$0.00");
        assert_eq!(format_currency(-10.0), "$-10.00");
        assert_eq!(format_currency(-100.5), "$-100.50");
    }

    #[test]
    fn format_currency_rounding_matches_js_to_fixed_behavior() {
        assert_eq!(format_currency(10.999), "$11.00");
        assert_eq!(format_currency(10.994), "$10.99");
        assert_eq!(format_currency(10.995), "$10.99");
        assert_eq!(format_currency(0.01), "$0.01");
        assert_eq!(format_currency(0.001), "$0.00");
        assert_eq!(format_currency(0.009), "$0.01");
    }

    #[test]
    fn format_models_display_multiline_formats_single_model() {
        let models = vec!["claude-sonnet-4-20250514".to_string()];
        assert_eq!(format_models_display_multiline(&models), "- sonnet-4");
    }

    #[test]
    fn format_models_display_multiline_formats_multiple_models() {
        let models = vec![
            "claude-sonnet-4-20250514".to_string(),
            "claude-opus-4-20250514".to_string(),
        ];
        assert_eq!(
            format_models_display_multiline(&models),
            "- opus-4\n- sonnet-4"
        );
    }

    #[test]
    fn format_models_display_multiline_removes_duplicates() {
        let models = vec![
            "claude-sonnet-4-20250514".to_string(),
            "claude-opus-4-20250514".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        ];
        assert_eq!(
            format_models_display_multiline(&models),
            "- opus-4\n- sonnet-4"
        );
    }

    #[test]
    fn format_models_display_multiline_handles_empty() {
        let models: Vec<String> = Vec::new();
        assert_eq!(format_models_display_multiline(&models), "");
    }

    #[test]
    fn format_models_display_multiline_handles_custom_models() {
        let models = vec![
            "custom-model".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        ];
        assert_eq!(
            format_models_display_multiline(&models),
            "- custom-model\n- sonnet-4"
        );
    }

    #[test]
    fn format_models_display_multiline_formats_claude_45() {
        let models = vec!["claude-sonnet-4-5-20250929".to_string()];
        assert_eq!(format_models_display_multiline(&models), "- sonnet-4-5");
    }

    #[test]
    fn format_models_display_multiline_formats_mixed_versions() {
        let models = vec![
            "claude-sonnet-4-20250514".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            "claude-opus-4-1-20250805".to_string(),
        ];
        assert_eq!(
            format_models_display_multiline(&models),
            "- opus-4-1\n- sonnet-4\n- sonnet-4-5"
        );
    }

    #[test]
    fn format_models_display_multiline_formats_pi_models() {
        let models = vec!["[pi] claude-opus-4-5".to_string()];
        assert_eq!(format_models_display_multiline(&models), "- [pi] opus-4-5");
    }

    #[test]
    fn format_models_display_multiline_formats_anthropic_prefix() {
        let models = vec!["anthropic/claude-opus-4.5".to_string()];
        assert_eq!(format_models_display_multiline(&models), "- opus-4.5");
    }

    #[test]
    fn format_models_display_multiline_formats_no_date_models() {
        let models = vec!["claude-opus-4-5".to_string()];
        assert_eq!(format_models_display_multiline(&models), "- opus-4-5");
    }

    #[test]
    fn format_models_display_multiline_formats_pi_anthropic_models() {
        let models = vec!["[pi] anthropic/claude-opus-4.5".to_string()];
        assert_eq!(format_models_display_multiline(&models), "- [pi] opus-4.5");
    }
}
