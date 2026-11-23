pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

pub fn set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_missing_variable_is_none_and_unset() {
        let name = "A_VARIABLE_NOBODY_SETS_8D1F";
        assert_eq!(super::var(name), None);
        assert!(!super::set(name));
        assert!(super::set("PATH"));
    }
}
