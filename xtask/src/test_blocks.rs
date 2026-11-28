const GATE: &str = "#[cfg(";

pub fn without_test_modules(source: &str) -> Result<String, String> {
    let mut code = source.to_owned();
    let mut from = 0;
    while let Some(found) = code[from..].find(GATE) {
        let gate = from + found;
        let Some(close) = code[gate..].find(")]").map(|at| gate + at + 2) else { break };
        from = close;
        if !gates_tests(&code[gate + GATE.len()..close - 2]) {
            continue;
        }
        let rest = code[close..].trim_start();
        let opens = rest.starts_with("mod ")
            && rest.find('{').is_some_and(|brace| rest.find(';').is_none_or(|semi| brace < semi));
        if !opens {
            continue;
        }
        let end = block_end(&code, close).ok_or_else(|| {
            let line = code[..gate].lines().count() + 1;
            format!("the test module at line {line} never closes its braces")
        })?;
        code.replace_range(gate..end, "");
        from = gate;
    }
    Ok(code)
}

fn gates_tests(condition: &str) -> bool {
    let words: Vec<&str> = condition.split(|letter: char| !(letter.is_alphanumeric() || letter == '_')).collect();
    words.contains(&"test") && !words.contains(&"not")
}

fn block_end(code: &str, from: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut at = from + code[from..].find('{')?;
    let mut depth = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            b'"' => at = string_end(bytes, at + 1)?,
            b'r' if raw_start(bytes, at).is_some() => at = raw_end(bytes, at)?,
            b'\'' => at = char_end(bytes, at),
            _ => {}
        }
        at += 1;
    }
    None
}

fn string_end(bytes: &[u8], mut at: usize) -> Option<usize> {
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return Some(at),
            _ => at += 1,
        }
    }
    None
}

fn raw_start(bytes: &[u8], at: usize) -> Option<usize> {
    let before = at.checked_sub(1).map(|before| bytes[before]);
    if before.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        return None;
    }
    let hashes = bytes[at + 1..].iter().take_while(|byte| **byte == b'#').count();
    (bytes.get(at + 1 + hashes) == Some(&b'"')).then_some(hashes)
}

fn raw_end(bytes: &[u8], at: usize) -> Option<usize> {
    let hashes = raw_start(bytes, at)?;
    let mut closing = vec![b'"'];
    closing.extend(std::iter::repeat_n(b'#', hashes));
    let body = at + hashes + 2;
    bytes[body..]
        .windows(closing.len())
        .position(|window| window == closing.as_slice())
        .map(|found| body + found + hashes)
}

fn char_end(bytes: &[u8], at: usize) -> usize {
    match (bytes.get(at + 1), bytes.get(at + 2), bytes.get(at + 3)) {
        (Some(b'\\'), _, _) => at + 1 + bytes[at + 1..].iter().position(|byte| *byte == b'\'').unwrap_or(0),
        (Some(_), Some(b'\''), _) => at + 2,
        _ => at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_module_goes_whatever_its_gate_and_its_strings_hold() {
        let source = "fn kept() {}\n#[cfg(all(test, unix))]\nmod tests { const A: &str = \"}\"; const B: &str = r#\"{ \"}\"#; const C: char = '}'; fn d<'a>(x: &'a str) {} }\n#[cfg(not(test))]\nmod real { fn e() {} }\n";
        let code = without_test_modules(source).expect("closes");
        assert!(code.contains("fn kept") && code.contains("mod real") && !code.contains("mod tests"), "{code}");
    }

    #[test]
    fn a_test_module_that_never_closes_is_an_error() {
        let refused = without_test_modules("fn a() {}\n#[cfg(test)]\nmod tests {\n  fn b() {\n");
        assert!(refused.is_err_and(|why| why.contains("line 2")));
    }
}
