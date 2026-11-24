pub fn label(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_bytes());
    let hex: String = digest.iter().take(6).map(|byte| format!("{byte:02x}")).collect();
    format!("{}-{hex}", brand::COMMAND)
}

pub fn marker(key: &str) -> String {
    format!("<!-- {}:{key} -->", brand::COMMAND)
}

pub fn marked(body: &str, key: &str) -> String {
    if key.is_empty() { body.to_owned() } else { format!("{body}\n\n{}", marker(key)) }
}

pub fn strip(body: &str) -> String {
    let open = format!("<!-- {}:", brand::COMMAND);
    let mut kept = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find(&open) {
        let Some(end) = rest[start..].find("-->") else { break };
        kept.push_str(&rest[..start]);
        rest = &rest[start + end + 3..];
    }
    kept.push_str(rest);
    kept.trim().to_owned()
}

pub fn carries(body: &str, key: &str) -> bool {
    !key.is_empty() && body.contains(&marker(key))
}

pub fn tag(key: &str) -> String {
    format!("[{}]", label(key))
}

pub fn tagged(body: &str, key: &str) -> String {
    if key.is_empty() { body.to_owned() } else { format!("{body}\n\n{}", tag(key)).trim_start().to_owned() }
}

pub fn carries_tag(body: &str, key: &str) -> bool {
    !key.is_empty() && (body.contains(&tag(key)) || carries(body, key))
}

pub fn branch(name: &str) -> String {
    let prefix = format!("{}/", brand::COMMAND);
    if name.starts_with(&prefix) { name.to_owned() } else { format!("{prefix}{name}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_is_an_invisible_comment_named_after_the_product() {
        let body = marked("hello", "item/step/0/1");
        assert!(body.starts_with("hello"));
        assert!(body.contains(&format!("<!-- {}:item/step/0/1 -->", brand::COMMAND)));
        assert!(carries(&body, "item/step/0/1"));
        assert!(!carries(&body, "item/step/0/2"));
        assert!(!carries(&body, ""));
        assert_eq!(marked("plain", ""), "plain");
    }

    #[test]
    fn stripping_removes_only_the_product_markers() {
        assert_eq!(strip(&marked("hello", "item/step/0/1")), "hello");
        assert_eq!(strip(&marked("", "k")), "");
        assert_eq!(strip("keep <!-- note --> this"), "keep <!-- note --> this");
        assert_eq!(strip(&format!("a {} b", marker("k"))), "a  b");
        assert_eq!(strip("plain"), "plain");
    }

    #[test]
    fn a_tag_is_plain_text_that_survives_rich_text_and_old_markers_still_count() {
        let body = tagged("descrição", "item/step/0/1");
        assert!(body.starts_with("descrição") && !body.contains('<'));
        assert!(carries_tag(&body, "item/step/0/1"));
        assert!(carries_tag(&format!("<p>{body}</p>"), "item/step/0/1"), "kept through HTML wrapping");
        assert!(!carries_tag(&body, "item/step/0/2"));
        assert!(carries_tag(&marked("x", "k"), "k"), "a marker written before still counts");
        assert_eq!(tagged("", "k"), tag("k"));
        assert_eq!(tagged("plain", ""), "plain");
    }

    #[test]
    fn keys_and_branches_carry_the_product_prefix() {
        assert_eq!(label("k1"), label("k1"));
        assert_ne!(label("k1"), label("k2"));
        assert!(label("k1").starts_with(&format!("{}-", brand::COMMAND)) && !label("k1").contains(' '));
        assert_eq!(branch("x"), format!("{}/x", brand::COMMAND));
        assert_eq!(branch(&branch("x")), branch("x"));
    }
}
