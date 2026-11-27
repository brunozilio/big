use super::{ShellCommand, SimpleCommand};

const UNSAFE: [char; 14] = ['$', '`', '>', '<', '(', ')', '{', '}', '\n', '\r', '\\', '!', '#', '\0'];

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ReadOnlyCommands {
    #[serde(default)]
    pub commands: Vec<ReadOnlyCommand>,
    #[serde(default)]
    pub sensitive: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ReadOnlyCommand {
    pub name: String,
    #[serde(default)]
    pub subcommands: Vec<String>,
    #[serde(default)]
    pub refused: Vec<String>,
}

impl ReadOnlyCommands {
    pub fn read(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }

    pub fn allow(&self, command: &str) -> bool {
        if command.trim().is_empty() || !plain_text(command) {
            return false;
        }
        let parsed = ShellCommand::parse(command);
        let commands = parsed.commands();
        parsed.flat() && !commands.is_empty() && commands.into_iter().all(|simple| self.allows_simple(simple))
    }

    fn allows_simple(&self, text: &str) -> bool {
        let Some(simple) = SimpleCommand::of(text) else { return false };
        if !simple.redirections.is_empty() {
            return false;
        }
        let words: Vec<String> = simple.words.iter().map(|word| word.replace(['\'', '"'], "")).collect();
        let Some(name) = simple.words.first() else { return false };
        let args = words.get(1..).unwrap_or_default();
        let Some(known) = self.commands.iter().find(|known| known.name == *name) else { return false };
        if !known.subcommands.is_empty() && args.first().is_none_or(|sub| !known.subcommands.contains(sub)) {
            return false;
        }
        args.iter().all(|arg| self.harmless(arg) && !known.refused.iter().any(|refused| refuses(refused, arg)))
    }

    fn harmless(&self, arg: &str) -> bool {
        let lower = arg.to_lowercase();
        let outside = arg.starts_with('/')
            || arg.starts_with('~')
            || arg.contains("..")
            || arg.contains("=/")
            || arg.contains("=~");
        !outside && !self.sensitive.iter().any(|word| lower.contains(&word.to_lowercase()))
    }
}

fn plain_text(command: &str) -> bool {
    if command.contains(UNSAFE) {
        return false;
    }
    let mut ands = command.split("&&");
    ands.all(|part| !part.contains('&'))
}

fn refuses(refused: &str, arg: &str) -> bool {
    if let Some(long) = refused.strip_prefix("--") {
        let Some(given) = arg.strip_prefix("--") else { return false };
        let given = given.split('=').next().unwrap_or_default();
        return !given.is_empty() && long.starts_with(given);
    }
    if let Some(flag) = refused.strip_prefix('-') {
        if flag.chars().count() == 1 {
            return arg.starts_with('-') && !arg.starts_with("--") && arg[1..].contains(flag);
        }
        return arg.starts_with(refused);
    }
    arg == refused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> ReadOnlyCommands {
        ReadOnlyCommands::read(
            r#"{
              "sensitive": ["secret", ".env", "token"],
              "commands": [
                {"name": "ls"},
                {"name": "cat"},
                {"name": "head"},
                {"name": "wc"},
                {"name": "grep"},
                {"name": "sort", "refused": ["-o", "--output", "--compress-program"]},
                {"name": "find", "refused": ["-exec", "-delete", "-fprint"]},
                {"name": "git", "subcommands": ["status", "log", "diff"], "refused": ["--output", "--ext-diff"]}
              ]
            }"#,
        )
        .unwrap_or_default()
    }

    #[test]
    fn plain_reads_and_pipes_between_them_are_allowed() {
        let declared = declared();
        for command in [
            "ls -la",
            "cat src/main.rs",
            "git status",
            "git log --oneline -20",
            "git diff HEAD~1 -- src",
            "grep -rn 'fn main' src | head -20",
            "ls && git status",
            "cat a.txt; wc -l b.txt",
            "find src -name '*.rs'",
            "sort -u names.txt",
        ] {
            assert!(declared.allow(command), "{command}");
        }
    }

    #[test]
    fn anything_that_could_write_run_or_hide_a_command_goes_to_the_reviewer() {
        let declared = declared();
        for command in [
            "",
            "rm -rf src",
            "ls; rm -rf src",
            "ls && rm -rf src",
            "ls || rm x",
            "cat a | sh",
            "cat a | bash -c x",
            "cat a | tee b",
            "cat $(echo rm)",
            "cat `rm x`",
            "cat a > b",
            "cat a >> b",
            "cat < a",
            "ls 2>/dev/null",
            "ls &",
            "ls & rm x",
            "ls |& cat",
            "ls\nrm x",
            "( rm x )",
            "{ ls; }",
            "if true; then ls; fi",
            "for f in *; do cat $f; done",
            "f() { rm x; }; f",
            "! ls",
            "X=1 ls",
            "/bin/ls",
            "\\rm x",
            "l\\s",
            "ca't' a",
            "cat /etc/passwd",
            "cat ~/.ssh/id_rsa",
            "cat ../outside",
            "cat .env",
            "grep -r SECRET_KEY .",
            "grep --file=/etc/x a",
            "find . -exec rm {} ;",
            "find . -execdir rm x",
            "find . -delete",
            "find . -fprint0 out",
            "sort -o out in",
            "sort -uo out in",
            "sort --output=out in",
            "sort --out=out in",
            "sort --compress-program=sh in",
            "git push",
            "git -c core.pager=sh log",
            "git -C .. status",
            "git log --output=x",
            "git diff --ext-diff",
            "git",
            "ls # ; rm x",
        ] {
            assert!(!declared.allow(command), "{command:?}");
        }
    }

    #[test]
    fn a_command_not_declared_is_never_allowed() {
        assert!(!ReadOnlyCommands::default().allow("ls"));
    }
}
