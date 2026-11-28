use std::collections::BTreeSet;
use std::path::Path;

const UPSTREAM: &str = "origin/main";

type Read = fn(&str) -> Result<BTreeSet<String>, String>;

struct Ran {
    ok: bool,
    out: String,
    err: String,
}

fn git(root: &Path, args: &[&str]) -> Result<Ran, String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("git does not run, so the published ids cannot be checked: {error}"))?;
    Ok(Ran {
        ok: output.status.success(),
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

fn required(root: &Path, args: &[&str]) -> Result<String, String> {
    let ran = git(root, args)?;
    if ran.ok { Ok(ran.out) } else { Err(format!("git {} failed: {}", args.join(" "), ran.err)) }
}

fn at(root: &Path, revision: &str, path: &str, read: Read) -> Result<BTreeSet<String>, String> {
    let listed = required(root, &["ls-tree", "--name-only", revision, "--", path])?;
    if listed.trim().is_empty() {
        return Ok(BTreeSet::new());
    }
    read(&required(root, &["show", &format!("{revision}:{path}")])?).map_err(|why| format!("{revision}: {why}"))
}

fn merge_base(root: &Path) -> Result<Option<String>, String> {
    if !git(root, &["rev-parse", "--verify", "--quiet", UPSTREAM])?.ok {
        return Ok(None);
    }
    let base = required(root, &["merge-base", "HEAD", UPSTREAM])?;
    Ok(Some(base.trim().to_owned()))
}

pub fn committed(root: &Path, path: &str, read: Read) -> Result<BTreeSet<String>, String> {
    let shallow = required(root, &["rev-parse", "--is-shallow-repository"])?;
    if shallow.trim() == "true" {
        return Err(format!("a shallow checkout cannot prove {path} only grew; fetch the history"));
    }
    let mut found = at(root, "HEAD", path, read)?;
    if let Some(base) = merge_base(root)? {
        found.extend(at(root, &base, path, read)?);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Result<BTreeSet<String>, String> {
        Ok(text.lines().map(str::to_owned).collect())
    }

    fn repo() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("root");
        for args in [&["init", "-q"][..], &["config", "user.email", "x@y"], &["config", "user.name", "x"], &["config", "commit.gpgsign", "false"]] {
            required(root.path(), args).expect("git");
        }
        root
    }

    fn commit(root: &Path, text: &str) {
        std::fs::write(root.join("ids.txt"), text).expect("write");
        required(root, &["add", "ids.txt"]).expect("add");
        required(root, &["commit", "-q", "-m", "c"]).expect("commit");
    }

    #[test]
    fn the_committed_ledger_is_read_from_head_and_a_missing_one_is_empty() {
        let root = repo();
        commit(root.path(), "a\nb\n");
        assert_eq!(committed(root.path(), "ids.txt", lines), lines("a\nb"));
        assert_eq!(committed(root.path(), "other.txt", lines), Ok(BTreeSet::new()));
    }

    #[test]
    fn without_a_repository_the_check_fails_closed() {
        let root = tempfile::tempdir().expect("root");
        assert!(committed(root.path(), "ids.txt", lines).is_err());
    }

    #[test]
    fn a_ledger_that_does_not_read_fails_closed() {
        let root = repo();
        commit(root.path(), "a\n");
        assert!(committed(root.path(), "ids.txt", |_| Err("broken".into())).is_err());
    }
}
