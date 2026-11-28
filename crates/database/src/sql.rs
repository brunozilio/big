pub fn for_postgres(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut chars = sql.chars().peekable();
    let mut quoted = false;
    let mut next = 1;
    while let Some(c) = chars.next() {
        if c == '\'' {
            quoted = !quoted;
            out.push(c);
            continue;
        }
        if c != '?' || quoted {
            out.push(c);
            continue;
        }
        let mut digits = String::new();
        while let Some(d) = chars.peek().copied().filter(char::is_ascii_digit) {
            digits.push(d);
            chars.next();
        }
        if digits.is_empty() {
            digits = next.to_string();
            next += 1;
        }
        out.push('$');
        out.push_str(&digits);
    }
    out
}

const TYPES: [(&str, &str); 3] = [("INTEGER", "BIGINT"), ("BLOB", "BYTEA"), ("REAL", "DOUBLE PRECISION")];

pub fn schema_for_postgres(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let upper = word.to_ascii_uppercase();
        match TYPES.iter().find(|(from, _)| *from == upper) {
            Some((_, to)) => out.push_str(to),
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in sql.replace("WITHOUT ROWID", "").chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}
