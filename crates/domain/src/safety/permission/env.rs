use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum EnvRule {
    Any(String),
    Valued { name: String, values: Vec<String> },
}

impl EnvRule {
    fn name(&self) -> &str {
        match self {
            Self::Any(name) | Self::Valued { name, .. } => name.trim(),
        }
    }

    pub(super) fn named(&self) -> bool {
        !self.name().is_empty()
    }

    pub(super) fn injects(&self, set: &Set) -> bool {
        let named = match self.name().strip_suffix('*') {
            Some(prefix) => set.name.starts_with(prefix),
            None => set.name == self.name(),
        };
        named
            && match self {
                Self::Any(_) => true,
                Self::Valued { values, .. } => set.value.split_whitespace().any(|token| flagged(token, values)),
            }
    }
}

fn flagged(token: &str, values: &[String]) -> bool {
    values.iter().map(|value| value.trim()).filter(|value| !value.is_empty()).any(|value| {
        let short = value.len() == 2 && value.starts_with('-') && !value.starts_with("--");
        token == value || token.starts_with(&format!("{value}=")) || (short && token.starts_with(value))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    pub name: String,
    pub value: String,
}

fn unquoted(text: &str) -> &str {
    text.trim_matches(|c| c == '"' || c == '\'')
}

pub(super) fn assignment(word: &str) -> Option<Set> {
    let (name, value) = unquoted(word).split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    let named = (first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    named.then(|| Set { name: name.to_owned(), value: unquoted(value).to_owned() })
}

pub(super) fn program(words: &[String]) -> Option<(usize, String)> {
    let at = words.iter().position(|word| assignment(word).is_none())?;
    let name = unquoted(&words[at]);
    Some((at, name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase()))
}

fn after_env(words: &[String]) -> Vec<Set> {
    let mut found = Vec::new();
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        if word == "-u" || word == "--unset" {
            rest.next();
        } else if let Some(set) = assignment(word) {
            found.push(set);
        } else if !word.starts_with('-') {
            break;
        }
    }
    found
}

fn carried(words: &[String], flags: &[&str]) -> Vec<Set> {
    let mut found = Vec::new();
    let mut rest = words.iter();
    while let Some(word) = rest.next() {
        if flags.contains(&word.as_str()) {
            found.extend(rest.next().and_then(|value| assignment(value)));
        } else if let Some((flag, value)) = word.split_once('=')
            && flags.contains(&flag)
        {
            found.extend(assignment(value));
        }
    }
    found
}

pub(super) fn assigned(words: &[String], carriers: &[(String, String)]) -> Vec<Set> {
    let mut found: Vec<Set> = words.iter().map_while(|word| assignment(word)).collect();
    let Some((at, program)) = program(words) else { return found };
    let rest = &words[at + 1..];
    match program.as_str() {
        "env" => found.extend(after_env(rest)),
        "export" => found.extend(rest.iter().filter_map(|word| assignment(word))),
        _ => {
            let flags: Vec<&str> =
                carriers.iter().filter(|(app, _)| *app == program).map(|(_, flag)| flag.as_str()).collect();
            if !flags.is_empty() {
                found.extend(carried(rest, &flags));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        crate::shell::SimpleCommand::of(text).map(|parsed| parsed.words).unwrap_or_default()
    }

    fn carriers() -> Vec<(String, String)> {
        vec![("docker".into(), "-e".into()), ("docker".into(), "--env".into())]
    }

    fn names(text: &str) -> Vec<String> {
        assigned(&words(text), &carriers()).into_iter().map(|set| format!("{}={}", set.name, set.value)).collect()
    }

    #[test]
    fn only_assignments_that_reach_the_program_count() {
        assert_eq!(names("NODE_OPTIONS=--require x node a.js"), ["NODE_OPTIONS=--require"]);
        assert_eq!(names("NODE_OPTIONS='--require ./x.js' node a.js"), ["NODE_OPTIONS=--require ./x.js"]);
        assert_eq!(
            names("env -i DYLD_INSERT_LIBRARIES=/tmp/x.dylib node a.js"),
            ["DYLD_INSERT_LIBRARIES=/tmp/x.dylib"]
        );
        assert_eq!(names("export NODE_OPTIONS=--import=x"), ["NODE_OPTIONS=--import=x"]);
        assert_eq!(names("docker run -e \"LD_PRELOAD=/x.so\" --env=A=1 img"), ["LD_PRELOAD=/x.so", "A=1"]);
        assert!(names("git commit -m \"docs: NODE_OPTIONS=x\"").is_empty());
        assert!(names("echo NODE_OPTIONS=--require").is_empty());
        assert!(names("podman run -e LD_PRELOAD=/x.so img").is_empty(), "only declared carriers pass env");
    }

    #[test]
    fn a_valued_rule_fires_only_on_a_value_that_loads_code() {
        let rule = EnvRule::Valued { name: "NODE_OPTIONS".into(), values: vec!["--require".into(), "-r".into()] };
        let set = |value: &str| Set { name: "NODE_OPTIONS".into(), value: value.into() };
        assert!(rule.injects(&set("--require x")));
        assert!(rule.injects(&set("--require=./x")));
        assert!(rule.injects(&set("-r ./x")));
        assert!(rule.injects(&set("-r./x")));
        assert!(!rule.injects(&set("--max-old-space-size=4096")));
        assert!(
            EnvRule::Any("DYLD_*".into()).injects(&Set { name: "DYLD_INSERT_LIBRARIES".into(), value: String::new() })
        );
    }
}
