#[test]
fn the_chat_s_review_commands_ship_as_skills() {
    for name in ["review", "code-review"] {
        let path = format!("{name}/SKILL.md");
        let skill = content::builtin(&format!("skills/{path}"));
        let text = String::from_utf8_lossy(skill.unwrap_or_default());
        assert!(text.contains(&format!("name: {name}\n")), "{path}: {text}");
    }
}

#[test]
fn every_shipped_skill_speaks_as_brand() {
    for (path, bytes) in content::builtin_files("skills/") {
        if path.ends_with("LICENSE.txt") || !path.ends_with(".md") {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        assert!(!brand::text(&text).contains("{{"), "{path}: an unknown placeholder");
        for other in ["Claude", "Anthropic", "claude.ai"] {
            assert!(!text.contains(other), "{path} names {other}");
        }
    }
}
