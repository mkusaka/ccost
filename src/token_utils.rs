#[derive(Debug, Clone, Copy)]
pub struct RawTokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct AggregatedTokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

pub fn get_total_tokens_from_raw(tokens: RawTokenCounts) -> u64 {
    tokens.input_tokens
        + tokens.output_tokens
        + tokens.cache_creation_input_tokens
        + tokens.cache_read_input_tokens
}

pub fn get_total_tokens_from_aggregated(tokens: AggregatedTokenCounts) -> u64 {
    tokens.input_tokens
        + tokens.output_tokens
        + tokens.cache_creation_tokens
        + tokens.cache_read_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_total_tokens_raw_sums_all_token_types() {
        let tokens = RawTokenCounts {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_input_tokens: 2000,
            cache_read_input_tokens: 300,
        };
        assert_eq!(get_total_tokens_from_raw(tokens), 3800);
    }

    #[test]
    fn get_total_tokens_aggregated_sums_all_token_types() {
        let tokens = AggregatedTokenCounts {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 2000,
            cache_read_tokens: 300,
        };
        assert_eq!(get_total_tokens_from_aggregated(tokens), 3800);
    }

    #[test]
    fn get_total_tokens_raw_handles_zero_values() {
        let tokens = RawTokenCounts {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        assert_eq!(get_total_tokens_from_raw(tokens), 0);
    }

    #[test]
    fn get_total_tokens_aggregated_handles_zero_values() {
        let tokens = AggregatedTokenCounts {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
        };
        assert_eq!(get_total_tokens_from_aggregated(tokens), 0);
    }
}
