pub const AGENTS_AT_ONCE: usize = 4;
pub const MCP_DEFER_OVER: usize = 20;

pub fn count_or(value: &str, default: usize) -> usize {
    value.trim().parse::<usize>().ok().filter(|count| *count > 0).unwrap_or(default)
}

pub fn agents_at_once() -> usize {
    count_or(brand::agent::PARALLEL_AGENTS, AGENTS_AT_ONCE)
}

pub fn mcp_defer_over() -> usize {
    count_or(brand::agent::MCP_DEFER_OVER, MCP_DEFER_OVER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_comes_from_the_config_and_an_unusable_one_falls_back_to_the_default() {
        assert_eq!(count_or(" 6 ", 4), 6);
        for unusable in ["", "0", "-2", "many"] {
            assert_eq!(count_or(unusable, 4), 4, "{unusable:?}");
        }
    }
}
