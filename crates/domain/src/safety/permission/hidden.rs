const BLACK_FLAG: char = '\u{1F3F4}';
const CANCEL_TAG: char = '\u{E007F}';

fn bidi(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn tag(c: char) -> bool {
    matches!(c, '\u{E0000}'..='\u{E007F}')
}

fn flagged(text: &str) -> Vec<bool> {
    let chars: Vec<char> = text.chars().collect();
    let mut hidden = vec![false; chars.len()];
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == BLACK_FLAG {
            let mut end = at + 1;
            while end < chars.len() && matches!(chars[end], '\u{E0020}'..='\u{E007E}') {
                end += 1;
            }
            if end > at + 1 && chars.get(end) == Some(&CANCEL_TAG) {
                at = end + 1;
                continue;
            }
        }
        hidden[at] = bidi(c) || tag(c);
        at += 1;
    }
    hidden
}

pub fn hidden(text: &str) -> Vec<char> {
    let mut found: Vec<char> = text.chars().zip(flagged(text)).filter(|(_, hidden)| *hidden).map(|(c, _)| c).collect();
    found.sort_unstable();
    found.dedup();
    found
}

pub fn revealed(text: &str) -> String {
    text.chars()
        .zip(flagged(text))
        .map(|(c, hidden)| if hidden { format!("<U+{:04X}>", u32::from(c)) } else { c.to_string() })
        .collect()
}

pub fn warned(text: &str) -> String {
    let found = hidden(text);
    if found.is_empty() {
        return text.to_owned();
    }
    let codes: Vec<String> = found.iter().map(|c| format!("U+{:04X}", u32::from(*c))).collect();
    format!(
        "Warning: this holds invisible or direction-changing characters ({}), so it may read differently from what runs. They are shown as <U+…> below.\n{}",
        codes.join(", "),
        revealed(text)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_direction_overrides_and_stray_tags_are_hidden() {
        for plain in [
            "echo \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} family",
            "echo \u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F} England",
            "echo \u{0645}\u{06CC}\u{200C}\u{062E}\u{0648}\u{0627}\u{0647}\u{0645}",
            "echo \u{0915}\u{094D}\u{200D}\u{0937}",
            "echo \u{200E}left\u{200F}right\u{FEFF}",
        ] {
            assert!(hidden(plain).is_empty(), "{plain:?}");
            assert_eq!(warned(plain), plain);
        }
        let flipped = "echo safe\u{202E}fdp.exe";
        assert_eq!(hidden(flipped), ['\u{202E}']);
        assert!(warned(flipped).contains("safe<U+202E>fdp.exe"));
        assert_eq!(hidden("ls \u{E0041}\u{E0042}"), ['\u{E0041}', '\u{E0042}'], "tags outside a flag");
        assert_eq!(hidden("a\u{2066}b\u{2069}"), ['\u{2066}', '\u{2069}']);
    }
}
