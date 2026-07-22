use num_format::{Locale, ToFormattedString};
use regex::Regex;
use std::sync::LazyLock;

static PI_MODEL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[pi\] (.+)$").expect("valid pi model regex"));
static ANTHROPIC_CLAUDE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^anthropic/claude-(\w+)-([\d.]+)$").expect("valid model regex"));
static CLAUDE_DATED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^claude-(\w+)-([\d-]+)-(\d{8})$").expect("valid dated model regex")
});
static CLAUDE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^claude-(\w+)-([\d-]+)$").expect("valid model regex"));

#[derive(Debug, Clone)]
pub struct UsageDataRow {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
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
    pub total_tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum TableMode {
    Full,
    Compact,
}

#[derive(Debug, Clone, Copy)]
pub enum TokenFormat {
    Exact,
    HumanReadable,
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

pub fn format_tokens(tokens: u64, format: TokenFormat) -> String {
    if matches!(format, TokenFormat::Exact) {
        return tokens.to_formatted_string(&Locale::en);
    }

    const UNITS: &[(u64, &str)] = &[(1_000, "K"), (1_000_000, "M"), (1_000_000_000, "B")];

    let Some(mut unit_index) = UNITS.iter().rposition(|(divisor, _)| tokens >= *divisor) else {
        return tokens.to_string();
    };

    let mut rounded = rounded_hundredths(tokens, UNITS[unit_index].0);
    if rounded >= 100_000 && unit_index + 1 < UNITS.len() {
        unit_index += 1;
        rounded = rounded_hundredths(tokens, UNITS[unit_index].0);
    }

    let whole = rounded / 100;
    let fraction = rounded % 100;
    let compact = match fraction {
        0 => whole.to_string(),
        value if value % 10 == 0 => format!("{whole}.{}", value / 10),
        value => format!("{whole}.{value:02}"),
    };
    let suffix = UNITS[unit_index].1;
    format!("{compact}{suffix}")
}

fn rounded_hundredths(tokens: u64, divisor: u64) -> u64 {
    let whole = tokens / divisor;
    let remainder = tokens % divisor;
    whole * 100 + (remainder * 100 + divisor / 2) / divisor
}

pub fn format_currency(amount: f64) -> String {
    if !amount.is_finite() {
        return format!("${amount:.2}");
    }

    let rounded = format!("{amount:.2}");
    let (sign, rest) = rounded
        .strip_prefix('-')
        .map_or(("", rounded.as_str()), |value| ("-", value));
    let (int_part, frac_part) = rest.split_once('.').expect("currency has two decimals");
    let grouped = int_part.parse::<u128>().map_or_else(
        |_| int_part.to_string(),
        |value| value.to_formatted_string(&Locale::en),
    );

    format!("${sign}{grouped}.{frac_part}")
}

fn format_model_name(model_name: &str) -> String {
    if let Some(caps) = PI_MODEL_RE.captures(model_name) {
        return format!("[pi] {}", format_model_name(&caps[1]));
    }

    if let Some(caps) = ANTHROPIC_CLAUDE_RE.captures(model_name) {
        return format!("{}-{}", &caps[1], &caps[2]);
    }

    if let Some(caps) = CLAUDE_DATED_RE.captures(model_name) {
        return format!("{}-{}", &caps[1], &caps[2]);
    }

    if let Some(caps) = CLAUDE_RE.captures(model_name) {
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
    token_format: TokenFormat,
) -> Vec<String> {
    match mode {
        TableMode::Full => vec![
            first_column_value.to_string(),
            format_models_display_multiline(&data.models_used),
            format_tokens(data.input_tokens, token_format),
            format_tokens(data.output_tokens, token_format),
            format_tokens(data.cache_creation_tokens, token_format),
            format_tokens(data.cache_read_tokens, token_format),
            format_tokens(data.total_tokens, token_format),
            format_currency(data.total_cost),
        ],
        TableMode::Compact => vec![
            first_column_value.to_string(),
            format_models_display_multiline(&data.models_used),
            format_tokens(data.input_tokens, token_format),
            format_tokens(data.output_tokens, token_format),
            format_currency(data.total_cost),
        ],
    }
}

pub fn build_totals_row(
    totals: &UsageDataRow,
    mode: TableMode,
    token_format: TokenFormat,
) -> Vec<String> {
    match mode {
        TableMode::Full => vec![
            "Total".to_string(),
            String::new(),
            format_tokens(totals.input_tokens, token_format),
            format_tokens(totals.output_tokens, token_format),
            format_tokens(totals.cache_creation_tokens, token_format),
            format_tokens(totals.cache_read_tokens, token_format),
            format_tokens(totals.total_tokens, token_format),
            format_currency(totals.total_cost),
        ],
        TableMode::Compact => vec![
            "Total".to_string(),
            String::new(),
            format_tokens(totals.input_tokens, token_format),
            format_tokens(totals.output_tokens, token_format),
            format_currency(totals.total_cost),
        ],
    }
}

pub fn build_breakdown_rows(
    breakdowns: &[ModelBreakdownRow],
    mode: TableMode,
    token_format: TokenFormat,
) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for breakdown in breakdowns {
        match mode {
            TableMode::Full => rows.push(vec![
                format!("  |- {}", format_model_name(&breakdown.model_name)),
                String::new(),
                format_tokens(breakdown.input_tokens, token_format),
                format_tokens(breakdown.output_tokens, token_format),
                format_tokens(breakdown.cache_creation_tokens, token_format),
                format_tokens(breakdown.cache_read_tokens, token_format),
                format_tokens(breakdown.total_tokens, token_format),
                format_currency(breakdown.cost),
            ]),
            TableMode::Compact => rows.push(vec![
                format!("  |- {}", format_model_name(&breakdown.model_name)),
                String::new(),
                format_tokens(breakdown.input_tokens, token_format),
                format_tokens(breakdown.output_tokens, token_format),
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
    fn format_tokens_uses_uppercase_compact_units() {
        assert_eq!(format_tokens(999, TokenFormat::HumanReadable), "999");
        assert_eq!(format_tokens(1_000, TokenFormat::HumanReadable), "1K");
        assert_eq!(format_tokens(1_234, TokenFormat::HumanReadable), "1.23K");
        assert_eq!(format_tokens(999_999, TokenFormat::HumanReadable), "1M");
        assert_eq!(
            format_tokens(1_234_567, TokenFormat::HumanReadable),
            "1.23M"
        );
        assert_eq!(
            format_tokens(69_960_297_352, TokenFormat::HumanReadable),
            "69.96B"
        );
    }

    #[test]
    fn format_tokens_preserves_exact_format_by_default() {
        assert_eq!(
            format_tokens(69_960_297_352, TokenFormat::Exact),
            "69,960,297,352"
        );
    }

    #[test]
    fn usage_rows_apply_human_readable_format_only_to_tokens() {
        let row = build_usage_row(
            "2026-07",
            &UsageDataRow {
                input_tokens: 1_234,
                output_tokens: 2_000_000,
                cache_creation_tokens: 3_000_000_000,
                cache_read_tokens: 999,
                total_tokens: 3_002_001_233,
                total_cost: 12.34,
                models_used: Vec::new(),
            },
            TableMode::Full,
            TokenFormat::HumanReadable,
        );

        assert_eq!(
            row,
            vec!["2026-07", "", "1.23K", "2M", "3B", "999", "3B", "$12.34"]
        );
    }

    #[test]
    fn format_currency_formats_amounts() {
        assert_eq!(format_currency(10.0), "$10.00");
        assert_eq!(format_currency(100.5), "$100.50");
        assert_eq!(format_currency(1234.56), "$1,234.56");
        assert_eq!(format_currency(53887.29), "$53,887.29");
    }

    #[test]
    fn format_currency_handles_zero_and_negative() {
        assert_eq!(format_currency(0.0), "$0.00");
        assert_eq!(format_currency(-10.0), "$-10.00");
        assert_eq!(format_currency(-100.5), "$-100.50");
        assert_eq!(format_currency(-1234.56), "$-1,234.56");
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
