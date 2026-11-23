const FENCE: &str = "```";

pub fn commands(criteria: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut fenced = false;
    for line in criteria.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(FENCE) {
            fenced = !fenced;
            continue;
        }
        let spans: Vec<&str> =
            if fenced { vec![trimmed.trim_start_matches("$ ")] } else { line.split('`').skip(1).step_by(2).collect() };
        for span in spans.into_iter().map(str::trim).filter(|span| !span.is_empty()) {
            if !found.iter().any(|known| known == span) {
                found.push(span.to_owned());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_are_the_criteria_s_code_spans_and_code_block_lines() {
        let text = "Run `cargo test --workspace` and `npm run build`.\n```sh\n$ make lint\n\npytest -q\n```\nDone.";
        assert_eq!(commands(text), ["cargo test --workspace", "npm run build", "make lint", "pytest -q"]);
        assert!(commands("no commands here").is_empty());
    }
}
