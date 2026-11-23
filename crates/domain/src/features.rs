pub fn on(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("true")
}

pub fn skill_on(skill: &str) -> bool {
    skill != "follow-pr" || on(brand::features::PR_FOLLOW)
}

#[cfg(test)]
mod tests {
    use brand::features as flags;

    use super::*;

    const ALL: [&str; 37] = [
        flags::REDACT_SECRETS,
        flags::BUDGET,
        flags::CACHE_WARNING,
        flags::ESCALATION,
        flags::PROJECT_CRITERIA,
        flags::AUTO_REVIEW,
        flags::FOLLOW_THROUGH,
        flags::AFFECTED_TESTS,
        flags::PR_FOLLOW,
        flags::SESSION_CLAIMS,
        flags::LOAD_ON_CALL,
        flags::OUTPUT_CLEANUP,
        flags::OUTPUT_FILTERS,
        flags::OUTPUT_SAVINGS,
        flags::BRANCHES,
        flags::GIT_OFFER,
        flags::TEAM_MEMORY,
        flags::LEARNING,
        flags::WEB_MAIN_CONTENT,
        flags::WEB_CLEANUP,
        flags::WEB_LINKS,
        flags::WEB_SMALL_PAGES_DIRECT,
        flags::WEB_FORMATS,
        flags::WEB_EMPTY_PAGE_ERROR,
        flags::CODE_MODE,
        flags::MEMORY_ANCHORS,
        flags::STRUCTURED_LEARNINGS,
        flags::LEARNING_CHECK,
        flags::BLAST_RADIUS,
        flags::CODE_MAP,
        flags::MEMORY_SEED,
        flags::MEMORY_SCORECARD,
        flags::MEMORY_SEARCH,
        flags::SCHEDULES,
        flags::PLUGINS,
        flags::TRACE,
        flags::CHANNELS,
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

    #[test]
    fn only_the_follow_pr_skill_has_a_flag() {
        assert!(skill_on("review"));
        assert_eq!(skill_on("follow-pr"), on(flags::PR_FOLLOW));
    }
}
