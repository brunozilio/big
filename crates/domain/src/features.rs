pub fn on(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("true")
}

#[cfg(test)]
mod tests {
    use brand::features as flags;

    use super::*;

    const ALL: [&str; 25] = [
        flags::REDACT_SECRETS,
        flags::BUDGET,
        flags::ESCALATION,
        flags::AUTO_REVIEW,
        flags::FOLLOW_THROUGH,
        flags::AFFECTED_TESTS,
        flags::SESSION_CLAIMS,
        flags::LOAD_ON_CALL,
        flags::OUTPUT_CLEANUP,
        flags::OUTPUT_FILTERS,
        flags::OUTPUT_SAVINGS,
        flags::TEAM_MEMORY,
        flags::WEB_MAIN_CONTENT,
        flags::WEB_CLEANUP,
        flags::WEB_LINKS,
        flags::WEB_SMALL_PAGES_DIRECT,
        flags::WEB_FORMATS,
        flags::WEB_EMPTY_PAGE_ERROR,
        flags::CODE_MODE,
        flags::MEMORY_ANCHORS,
        flags::STRUCTURED_LEARNINGS,
        flags::BLAST_RADIUS,
        flags::CODE_MAP,
        flags::MEMORY_SCORECARD,
        flags::MEMORY_SEARCH,
    ];

    #[test]
    fn a_flag_is_on_only_when_the_config_says_true() {
        assert!(on("true") && on(" TRUE "));
        assert!(!on("false") && !on("") && !on("yes"));
    }

    #[test]
    fn every_flag_in_the_config_is_true_or_false() {
        for value in ALL {
            assert!(["true", "false"].contains(&value.trim()), "{value}");
        }
    }
}
