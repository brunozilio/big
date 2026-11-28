pub type Pairs = Vec<(String, String)>;

pub fn values(text: &str) -> (Pairs, Vec<(String, Pairs)>) {
    let mut top = Vec::new();
    let mut sections: Vec<(String, Pairs)> = Vec::new();
    for line in text.lines().map(|line| line.split_once('#').map_or(line, |(before, _)| before).trim()) {
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            sections.push((name.trim().to_owned(), Vec::new()));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let pair = (key.trim().to_owned(), value.trim().trim_matches('"').to_owned());
        match sections.last_mut() {
            Some((_, pairs)) => pairs.push(pair),
            None => top.push(pair),
        }
    }
    (top, sections)
}

fn put(pairs: &mut Pairs, (key, value): (String, String)) {
    match pairs.iter_mut().find(|(name, _)| *name == key) {
        Some(found) => found.1 = value,
        None => pairs.push((key, value)),
    }
}

pub fn overlaid(top: &mut Pairs, sections: &mut Vec<(String, Pairs)>, overlay: &str) {
    let (more_top, more_sections) = values(overlay);
    for pair in more_top {
        put(top, pair);
    }
    for (section, pairs) in more_sections {
        let index = match sections.iter().position(|(name, _)| *name == section) {
            Some(index) => index,
            None => {
                sections.push((section, Vec::new()));
                sections.len() - 1
            }
        };
        for pair in pairs {
            put(&mut sections[index].1, pair);
        }
    }
}
