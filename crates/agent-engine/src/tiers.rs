use std::sync::Arc;

use domain::model_roles::Tiers;

const FILE: &str = "model-tiers.ts";
const EXPORT: &str = "tiers";

pub fn current() -> Arc<Tiers> {
    let tiers = Arc::new(Tiers::read(&content::value(FILE, EXPORT).unwrap_or_default()));
    Tiers::declare(Arc::clone(&tiers));
    tiers
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shipped_content_declares_the_four_tiers_with_main_as_the_session_s_model() {
        let tiers = super::current();
        let names: Vec<&str> = tiers.all().iter().map(|spec| spec.name.as_str()).collect();
        assert_eq!(names, ["main", "plan", "explore", "small"]);
        assert!(tiers.find("main").is_some_and(|spec| spec.session));
        assert_eq!(tiers.find("explore").and_then(|spec| spec.fallback.as_deref()), Some("small"));
        assert!(tiers.all().iter().all(|spec| !spec.about.is_empty()));
        assert_eq!(
            domain::model_roles::ModelRole::parse("small").map(|role| role.name().to_owned()),
            Some("small".into())
        );
    }
}
