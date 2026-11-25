include!(concat!(env!("OUT_DIR"), "/brand.rs"));

pub fn env(key: &str) -> String {
    format!("{ENV}_{key}")
}

pub fn section(name: &str) -> &'static [(&'static str, &'static str)] {
    SECTIONS.iter().find(|(section, _)| *section == name).map_or(&[], |(_, pairs)| pairs)
}

pub fn text(template: &str) -> String {
    if !template.contains("{{") {
        return template.to_owned();
    }
    [
        ("{{NAME}}", NAME),
        ("{{COMMAND}}", COMMAND),
        ("{{DIR}}", DIR),
        ("{{INSTRUCTIONS}}", INSTRUCTIONS),
        ("{{LOCAL_INSTRUCTIONS}}", LOCAL_INSTRUCTIONS),
        ("{{ENV}}", ENV),
        ("{{COMPANY}}", COMPANY),
    ]
    .iter()
    .fold(template.to_owned(), |text, (placeholder, value)| text.replace(placeholder, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_derive_from_the_command() {
        assert_eq!(DIR, format!(".{COMMAND}"));
        assert_eq!(ENV, COMMAND.to_ascii_uppercase().replace('-', "_"));
        assert_eq!(INSTRUCTIONS, format!("{ENV}.md"));
        assert_eq!(env("PROJECT_DIR"), format!("{ENV}_PROJECT_DIR"));
    }

    #[test]
    fn the_logo_and_favicon_are_pngs() {
        assert!(LOGO.starts_with(b"\x89PNG"));
        assert!(FAVICON.starts_with(b"\x89PNG"));
    }

    #[test]
    fn a_section_lists_its_pairs_and_an_unknown_one_is_empty() {
        assert!(section("server").iter().any(|(key, _)| *key == "port"));
        assert!(section("nope").is_empty());
    }

    #[test]
    fn placeholders_are_filled_and_other_braces_kept() {
        let filled = text("Run `{{COMMAND}} login`; {{NAME}} reads {{DIR}}/{{INSTRUCTIONS}}. {x}");
        assert_eq!(filled, format!("Run `{COMMAND} login`; {NAME} reads {DIR}/{INSTRUCTIONS}. {{x}}"));
        assert_eq!(text("plain"), "plain");
    }
}

#[cfg(test)]
mod literal_tests {
    #[test]
    fn each_name_is_also_a_literal() {
        const JOINED: &str = concat!(name!(), " ", command!(), " ", dir!(), " ", env_prefix!());
        assert_eq!(JOINED, format!("{} {} {} {}", super::NAME, super::COMMAND, super::DIR, super::ENV));
    }
}
