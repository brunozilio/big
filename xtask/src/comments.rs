use std::path::Path;

fn scripted(path: &str) -> bool {
    path.starts_with("content/") && [".ts", ".js"].iter().any(|extension| path.ends_with(extension))
}

pub fn checked(path: &str) -> bool {
    !path.starts_with(".claude/skills/")
        && ([".rs", ".sh", ".py", ".toml"].iter().any(|extension| path.ends_with(extension)) || scripted(path))
}

pub fn lines(path: &str, source: &str) -> Vec<usize> {
    if path.ends_with(".rs") {
        rust(source)
    } else if scripted(path) {
        script(source)
    } else {
        hashes(source)
    }
}

fn script(source: &str) -> Vec<usize> {
    let mut found = Vec::new();
    script_until(source, 0, false, &mut found);
    found.dedup();
    found
}

fn script_until(source: &str, start: usize, in_braces: bool, found: &mut Vec<usize>) -> usize {
    let bytes = source.as_bytes();
    let line_of = |at: usize| source[..at].matches('\n').count() + 1;
    let mut depth = 0usize;
    let mut at = start;
    while at < bytes.len() {
        let rest = &source[at..];
        match bytes[at] {
            b'/' if rest.starts_with("//") => {
                found.push(line_of(at));
                at += rest.find('\n').unwrap_or(rest.len());
            }
            b'/' if rest.starts_with("/*") => {
                found.push(line_of(at));
                at += rest.find("*/").map_or(rest.len(), |end| end + 2);
            }
            b'"' | b'\'' => at += closed(rest, bytes[at]),
            b'`' => at = template(source, at + 1, found),
            b'/' if starts_regex(&source[..at]) => at += regex(rest),
            b'{' => {
                depth += 1;
                at += 1;
            }
            b'}' if in_braces && depth == 0 => return at + 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                at += 1;
            }
            _ => at += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    at
}

fn starts_regex(before: &str) -> bool {
    let trimmed = before.trim_end();
    let last = trimmed.chars().last();
    let keyword = ["return", "typeof", "case", "in", "of"].iter().any(|word| {
        trimmed.strip_suffix(word).is_some_and(|head| !head.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
    });
    keyword || last.is_none_or(|c| "(,=:[!&|?{};+-*%<>~^".contains(c))
}

fn regex(rest: &str) -> usize {
    let mut class = false;
    let mut escaped = false;
    for (index, byte) in rest.bytes().enumerate().skip(1) {
        match byte {
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            b'\n' => return index,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => return index + 1,
            _ => {}
        }
    }
    rest.len()
}

fn closed(rest: &str, quote: u8) -> usize {
    let mut escaped = false;
    for (index, byte) in rest.bytes().enumerate().skip(1) {
        match byte {
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            b'\n' => return index,
            _ if byte == quote => return index + 1,
            _ => {}
        }
    }
    rest.len()
}

fn template(source: &str, start: usize, found: &mut Vec<usize>) -> usize {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'`' => return at + 1,
            b'$' if bytes.get(at + 1) == Some(&b'{') => at = script_until(source, at + 2, true, found),
            _ => at += 1,
        }
    }
    at
}

fn hashes(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(index, line)| {
            let line = line.trim_start();
            line.starts_with('#') && !(*index == 0 && line.starts_with("#!"))
        })
        .map(|(index, _)| index + 1)
        .collect()
}

fn rust(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    let line_of = |at: usize| source[..at].matches('\n').count() + 1;
    while at < bytes.len() {
        let rest = &source[at..];
        if rest.starts_with("//") {
            found.push(line_of(at));
            at += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            found.push(line_of(at));
            at += block_end(rest);
        } else if let Some(length) = raw_string(rest, at == 0 || !word(bytes[at - 1])) {
            at += length;
        } else if bytes[at] == b'"' {
            at += quoted(rest);
        } else if bytes[at] == b'\'' {
            at += char_literal(rest).unwrap_or(1);
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    found.dedup();
    found
}

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn block_end(rest: &str) -> usize {
    let mut depth = 0;
    let mut at = 0;
    while at < rest.len() {
        if rest[at..].starts_with("/*") {
            depth += 1;
            at += 2;
        } else if rest[at..].starts_with("*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += rest[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    rest.len()
}

fn raw_string(rest: &str, starts_word: bool) -> Option<usize> {
    if !starts_word {
        return None;
    }
    let after = rest.strip_prefix("br").or_else(|| rest.strip_prefix('r'))?;
    let hashes = after.len() - after.trim_start_matches('#').len();
    let body = after[hashes..].strip_prefix('"')?;
    let close = format!("\"{}", "#".repeat(hashes));
    let end = body.find(&close)?;
    Some(rest.len() - body.len() + end + close.len())
}

fn quoted(rest: &str) -> usize {
    let mut escaped = false;
    for (index, c) in rest.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return index + 1,
            _ => {}
        }
    }
    rest.len()
}

fn char_literal(rest: &str) -> Option<usize> {
    let mut chars = rest.char_indices().skip(1);
    let (_, first) = chars.next()?;
    if first == '\\' {
        let close = rest[2..].find('\'')?;
        return (close <= 10).then_some(close + 3);
    }
    let (index, next) = chars.next()?;
    (next == '\'').then_some(index + 1)
}

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let listed = std::process::Command::new("git").arg("ls-files").current_dir(root).output()?;
    let mut found = Vec::new();
    for path in String::from_utf8_lossy(&listed.stdout).lines().filter(|path| checked(path)) {
        let Ok(source) = std::fs::read_to_string(root.join(path)) else { continue };
        if !lines(path, &source).is_empty() {
            found.push(format!("comment {path}"));
        }
    }
    Ok(found)
}

pub fn check(paths: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut report = Vec::new();
    for path in paths.iter().filter(|path| checked(path)) {
        let Ok(source) = std::fs::read_to_string(path) else { continue };
        for line in lines(path, &source) {
            report.push(format!("{path}:{line}: {}", source.lines().nth(line - 1).unwrap_or_default().trim()));
        }
    }
    if report.is_empty() {
        return Ok(());
    }
    eprintln!("Comments are not allowed in code (CLAUDE.md). Remove them:\n{}", report.join("\n"));
    Err("comments found".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_comments_are_found_and_strings_are_not() {
        let source = "fn a() {}\n// one\nlet url = \"https://x\";\nlet c = '/';\n/// two\nlet r = r#\"// raw\"#;\nlet x = 1; /* three */\nfn b<'a>(s: &'a str) {}\n";
        assert_eq!(rust(source), [2, 5, 7]);
        assert!(rust("let s = \"a \\\" // b\";\nlet t = b\"//\";\n").is_empty());
    }

    #[test]
    fn script_comments_in_content_are_found_and_strings_templates_and_urls_are_not() {
        let source = "const a = \"https://x\";\n// one\nconst t = `a ${ { b: \"//\" }.b } // not`;\nconst c = '/*';\nlet x = 1; /* two */\nconst r = /https?:\\/\\//;\n";
        assert_eq!(lines("content/tools/A.ts", source), [2, 5]);
        assert!(checked("content/tools/A.js") && !checked("crates/page/app.js"));
        assert!(lines("content/tools/B.ts", "const n = `${`${1}`}`;\n").is_empty());
    }

    #[test]
    fn hash_comments_spare_the_shebang() {
        assert_eq!(hashes("#!/bin/sh\n# note\necho '#'\n  # indented\n"), [2, 4]);
        assert!(lines("Cargo.toml", "[package]\nname = \"x\"\n").is_empty());
        assert!(!checked("README.md"));
    }
}
