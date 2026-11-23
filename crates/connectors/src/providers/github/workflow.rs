#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowJob {
    pub key: String,
    pub name: Option<String>,
    pub needs: Vec<String>,
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn meaningful(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with('#')
}

fn unquoted(text: &str) -> String {
    let text = text.split(" #").next().unwrap_or_default().trim();
    text.trim_matches(|c| c == '"' || c == '\'').to_owned()
}

fn inline_list(value: &str) -> Vec<String> {
    let value = value.trim();
    match value.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        Some(items) => items.split(',').map(unquoted).filter(|item| !item.is_empty()).collect(),
        None if value.is_empty() => Vec::new(),
        None => vec![unquoted(value)],
    }
}

fn jobs_block(yaml: &str) -> Vec<&str> {
    let lines: Vec<&str> = yaml.lines().filter(|line| meaningful(line)).collect();
    let Some(start) = lines.iter().position(|line| indent(line) == 0 && line.trim_end() == "jobs:") else {
        return Vec::new();
    };
    lines[start + 1..].iter().take_while(|line| indent(line) > 0).copied().collect()
}

pub fn jobs(yaml: &str) -> Vec<WorkflowJob> {
    let block = jobs_block(yaml);
    let Some(job_indent) = block.first().map(|line| indent(line)) else { return Vec::new() };
    let mut found: Vec<WorkflowJob> = Vec::new();
    let mut property_indent: Option<usize> = None;
    let mut in_needs = false;
    for line in block {
        let depth = indent(line);
        let trimmed = line.trim();
        if depth == job_indent {
            found.push(WorkflowJob { key: unquoted(trimmed.trim_end_matches(':')), ..WorkflowJob::default() });
            property_indent = None;
            in_needs = false;
            continue;
        }
        let Some(job) = found.last_mut() else { continue };
        let level = *property_indent.get_or_insert(depth);
        if depth == level {
            in_needs = false;
            if let Some(value) = trimmed.strip_prefix("name:") {
                job.name = Some(unquoted(value)).filter(|name| !name.is_empty());
            } else if let Some(value) = trimmed.strip_prefix("needs:") {
                job.needs = inline_list(value);
                in_needs = job.needs.is_empty();
            }
        } else if in_needs && let Some(item) = trimmed.strip_prefix("- ") {
            job.needs.push(unquoted(item));
        }
    }
    found
}

pub fn matches(job: &WorkflowJob, shown: &str) -> bool {
    let base = shown.split(" / ").next().unwrap_or(shown);
    let base = base.split(" (").next().unwrap_or(base);
    let named = job.name.as_deref().filter(|name| !name.contains("${{"));
    base == job.key || named.is_some_and(|name| name == base || name == shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "name: ci\non:\n  push:\n\njobs:\n  build:\n    name: Build app\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo\n        name: x\n  test:\n    needs: build\n    strategy:\n      matrix:\n        os: [a, b]\n  # comment\n  deploy:\n    needs: [build, \"test\"]\n  notify:\n    needs:\n      - deploy\n      - test\n    steps:\n      - run: echo\nenv:\n  A: 1\n";

    #[test]
    fn needs_are_read_in_every_yaml_form() {
        let found = jobs(YAML);
        let keys: Vec<&str> = found.iter().map(|job| job.key.as_str()).collect();
        assert_eq!(keys, ["build", "test", "deploy", "notify"]);
        assert_eq!(found[0].name.as_deref(), Some("Build app"));
        assert!(found[0].needs.is_empty());
        assert_eq!(found[1].needs, ["build"]);
        assert_eq!(found[2].needs, ["build", "test"]);
        assert_eq!(found[3].needs, ["deploy", "test"]);
    }

    #[test]
    fn api_job_names_match_by_name_key_or_matrix_prefix() {
        let found = jobs(YAML);
        assert!(matches(&found[0], "Build app"));
        assert!(matches(&found[1], "test (a)"));
        assert!(matches(&found[2], "deploy / call"));
        assert!(!matches(&found[1], "build"));
        assert!(jobs("name: x\n").is_empty());
    }
}
