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

pub fn carries(body: &str, key: &str) -> bool {
    !key.is_empty() && body.contains(&marker(key))
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
    fn keys_and_branches_carry_the_product_prefix() {
        assert_eq!(label("k1"), label("k1"));
        assert_ne!(label("k1"), label("k2"));
        assert!(label("k1").starts_with(&format!("{}-", brand::COMMAND)) && !label("k1").contains(' '));
        assert_eq!(branch("x"), format!("{}/x", brand::COMMAND));
        assert_eq!(branch(&branch("x")), branch("x"));
    }
}
