use serde::{Deserialize, Serialize};

use super::Grant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggested {
    pub grant: Grant,
    pub why: String,
}

const DANGEROUS: &[&str] = &[
    "rm", "rmdir", "sudo", "su", "dd", "mkfs", "chmod", "chown", "kill", "killall", "pkill", "shutdown", "reboot",
    "curl", "wget", "ssh", "scp", "rsync", "nc", "eval", "exec", "sh", "bash", "zsh", "xargs", "env", "mv", "cp",
];

const RUNNERS: &[&str] = &[
    "git",
    "cargo",
    "npm",
    "npx",
    "bun",
    "bunx",
    "yarn",
    "pnpm",
    "docker",
    "kubectl",
    "aws",
    "gh",
    "python",
    "python3",
    "node",
    "deno",
    "go",
    "make",
    "just",
    "terraform",
    "pip",
    "uv",
];

const HARMFUL_WORDS: &[&str] = &[
    "push",
    "reset",
    "clean",
    "rebase",
    "publish",
    "deploy",
    "destroy",
    "delete",
    "drop",
    "apply",
    "prune",
    "uninstall",
    "rm",
    "restore",
    "filter-branch",
    "force",
    "-f",
    "--force",
    "-D",
];

impl Grant {
    pub fn parse(rule: &str) -> Option<Self> {
        let text = rule.trim();
        if text.is_empty() {
            return None;
        }
        if text == "Bash" || text == "Bash(*)" {
            return Some(Self::Command("*".to_owned()));
        }
        let called = text.strip_suffix(')').and_then(|open| open.split_once('('));
        let grant = match called {
            Some(("Bash", inner)) => Self::Command(inner.trim().trim_end_matches(":*").trim().to_owned()),
            Some(("Site" | "WebFetch", inner)) => Self::Site(inner.trim().trim_start_matches("domain:").to_owned()),
            _ => Self::Tool(text.to_owned()),
        };
        grant.tidy()
    }

    pub fn rule(&self) -> String {
        match self {
            Self::Command(command) if command == "*" => "Bash".to_owned(),
            Self::Command(command) => format!("Bash({command}:*)"),
            Self::Site(site) => format!("Site({site})"),
            Self::Tool(tool) => tool.clone(),
        }
    }

    pub fn safe_to_suggest(&self) -> bool {
        match self {
            Self::Command(command) => {
                let words: Vec<&str> = command.split_whitespace().collect();
                let first = words.first().copied().unwrap_or("*");
                let lone_runner = words.len() == 1 && RUNNERS.contains(&first);
                first != "*"
                    && !DANGEROUS.contains(&first)
                    && !lone_runner
                    && !words.iter().any(|word| HARMFUL_WORDS.contains(word))
            }
            Self::Site(site) => site != "*",
            Self::Tool(tool) => match tool.strip_suffix('*') {
                Some(prefix) => prefix.matches("__").count() >= 2,
                None => tool.starts_with("mcp__"),
            },
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    rules: Vec<Given>,
}

#[derive(Deserialize)]
struct Given {
    rule: String,
    #[serde(default)]
    why: String,
}

pub fn parse_suggestions(answer: &str) -> Vec<Suggested> {
    let (Some(start), Some(end)) = (answer.find('{'), answer.rfind('}')) else { return Vec::new() };
    let Some(json) = answer.get(start..=end) else { return Vec::new() };
    let given = serde_json::from_str::<Answer>(json).map(|answer| answer.rules).unwrap_or_default();
    let mut kept: Vec<Suggested> = Vec::new();
    for Given { rule, why } in given {
        let Some(grant) = Grant::parse(&rule) else { continue };
        if grant.safe_to_suggest() && !kept.iter().any(|known| known.grant == grant) {
            kept.push(Suggested { grant, why: why.trim().to_owned() });
        }
    }
    kept
}
