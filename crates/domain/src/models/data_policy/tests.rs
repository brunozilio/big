use super::*;

fn handling(region: Option<&str>, training: Option<bool>, retention_days: Option<u32>) -> Handling {
    Handling { region: region.map(str::to_owned), training, retention_days }
}

fn in_regions(regions: &[&str]) -> Requires {
    Requires { regions: Some(regions.iter().map(|region| (*region).to_owned()).collect()), ..Requires::default() }
}

#[test]
fn nothing_required_accepts_anything_even_unknown() {
    let requires = Requires::default();
    assert!(requires.is_empty());
    assert!(requires.unmet(&Handling::default()).is_empty());
    assert!(requires.unmet(&handling(Some("us-east-1"), Some(true), Some(400))).is_empty());
}

#[test]
fn a_region_matches_itself_and_its_subregions_only() {
    let eu = in_regions(&["eu"]);
    assert!(eu.unmet(&handling(Some("eu"), None, None)).is_empty());
    assert!(eu.unmet(&handling(Some("EU-central-1"), None, None)).is_empty());
    assert_eq!(
        eu.unmet(&handling(Some("us-east-1"), None, None)),
        [Unmet::Region { wanted: vec!["eu".into()], declared: Some("us-east-1".into()) }]
    );
    assert_eq!(eu.unmet(&handling(Some("europe"), None, None)).len(), 1);
}

#[test]
fn an_unknown_declaration_never_meets_a_requirement() {
    let strict = Requires { regions: Some(vec!["eu".into()]), no_training: true, retention_days: Some(30) };
    let unmet = strict.unmet(&Handling::default());
    assert_eq!(unmet.len(), 3);
    assert_eq!(joined(&unmet), "region unknown is not eu; training on the data is unknown; retention is unknown");
}

#[test]
fn training_and_retention_are_checked_against_what_is_declared() {
    let strict = Requires { regions: None, no_training: true, retention_days: Some(30) };
    assert!(strict.unmet(&handling(None, Some(false), Some(0))).is_empty());
    assert!(strict.unmet(&handling(None, Some(false), Some(30))).is_empty());
    let unmet = strict.unmet(&handling(None, Some(true), Some(31)));
    assert_eq!(joined(&unmet), "trains on the data; keeps data 31 days, more than 30");
}

#[test]
fn the_config_section_reads_lists_and_blanks() {
    assert!(Requires::read(&[("region", ""), ("training", ""), ("retention_days", "")]).is_empty());
    let read = Requires::read(&[("region", "eu local"), ("training", "no"), ("retention_days", "0")]);
    assert_eq!(
        read,
        Requires { regions: Some(vec!["eu".into(), "local".into()]), no_training: true, retention_days: Some(0) }
    );
}

#[test]
fn combining_keeps_the_stricter_of_each() {
    let config =
        Requires { regions: Some(vec!["eu".into(), "local".into()]), no_training: false, retention_days: Some(30) };
    let space = Requires { regions: Some(vec!["eu-central-1".into()]), no_training: true, retention_days: Some(7) };
    let both = config.clone().stricter(&space);
    assert_eq!(
        both,
        Requires { regions: Some(vec!["eu-central-1".into()]), no_training: true, retention_days: Some(7) }
    );
    assert_eq!(config.clone().stricter(&Requires::default()), config);
    assert_eq!(Requires::default().stricter(&space), space);
}

#[test]
fn disjoint_regions_leave_nothing_allowed() {
    let both = in_regions(&["eu"]).stricter(&in_regions(&["us"]));
    assert_eq!(both.regions, Some(Vec::new()));
    assert_eq!(joined(&both.unmet(&handling(Some("eu"), None, None))), "no region meets every requirement");
}

#[test]
fn requirements_round_trip_as_camel_case_json() {
    let requires = Requires { regions: Some(vec!["eu".into()]), no_training: true, retention_days: Some(0) };
    let json = serde_json::to_value(&requires).expect("serialized");
    assert_eq!(json, serde_json::json!({ "regions": ["eu"], "noTraining": true, "retentionDays": 0 }));
    assert_eq!(serde_json::from_value::<Requires>(json).expect("read"), requires);
    assert_eq!(serde_json::to_value(Requires::default()).expect("serialized"), serde_json::json!({}));
}
