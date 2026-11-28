mod declaration;
mod disclosed;
mod vocabulary;

pub use declaration::{Declaration, META, Operation, Problem, Profile, REQUIRED, checked, hinted, values_at};
pub use disclosed::Disclosed;
pub use vocabulary::{Initial, Kind, Permission, Reversible, Risk, Visibility};

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn a_declaration_names_its_effects_and_finds_the_recipients_in_the_arguments() {
        let declared: Declaration = serde_json::from_value(json!({
            "effects": ["send-external", "pii"], "risk": "high", "reversible": false, "untrusted": false,
            "sensitive": true, "recipients": ["to", "cc"], "freeText": ["body"], "permission": { "alwaysAsk": true },
        }))
        .expect("declaration");
        assert_eq!(declared.profile.reversible, Reversible::No);
        let input =
            json!({ "to": ["ana@example.com", " "], "cc": ["rui@example.com", "ana@example.com"], "body": "oi" });
        let operation = declared.operation("s", &input).expect("acts");
        assert_eq!(operation.recipients, ["ana@example.com", "rui@example.com"]);
        assert!(operation.strict());
        let disclosed = Disclosed::of([&operation]);
        assert_eq!((disclosed.effects, disclosed.risk), (vec![Kind::SendExternal, Kind::Pii], Some(Risk::High)));
        let profile = json!({ "effects": ["read", "pii"], "risk": "low", "reversible": true, "untrusted": true, "sensitive": true });
        let reading: Declaration = serde_json::from_value(profile.clone()).expect("read");
        assert_eq!(reading.operation("s", &input), None);
        let mut refused = profile;
        refused["permission"] = json!({ "default": "deny" });
        let refused: Declaration = serde_json::from_value(refused).expect("refused");
        assert!(refused.operation("s", &input).is_some_and(|operation| operation.refused()));
        assert!(
            serde_json::from_value::<Declaration>(json!({ "effects": ["read"] })).is_err(),
            "no profile, no default"
        );
        let mut loose =
            json!({ "effects": ["write"], "risk": "low", "reversible": true, "untrusted": false, "sensitive": false });
        loose["permission"] = json!({ "default": "allow" });
        let loose: Declaration = serde_json::from_value(loose).expect("allow is read");
        let operation = loose.operation("s", &input).expect("acts");
        assert!(!operation.refused() && !operation.strict(), "allow is asked like ask: content only tightens");
    }

    #[test]
    fn a_profile_is_checked_attribute_by_attribute() {
        let full = json!({ "effects": ["write"], "risk": "low", "reversible": "compensate", "untrusted": false, "sensitive": false });
        assert_eq!(checked(&full).map(|profile| profile.reversible), Ok(Reversible::Compensate));
        let mut missing = full.clone();
        missing["sensitive"] = Value::Null;
        assert_eq!(checked(&missing), Err(Problem::Missing("sensitive")));
        let empty = json!({ "effects": [], "risk": "low", "reversible": true, "untrusted": false, "sensitive": false });
        assert_eq!(checked(&empty), Err(Problem::NoEffects));
        let mut personal = full.clone();
        personal["effects"] = json!(["read", "pii"]);
        assert!(matches!(checked(&personal), Err(Problem::Invalid { attribute: "sensitive", .. })));
        let mut odd = full;
        odd["reversible"] = json!("maybe");
        assert!(matches!(checked(&odd), Err(Problem::Invalid { .. })));
        assert_eq!(serde_json::to_value(Reversible::Compensate).ok(), Some(json!("compensate")));
    }

    #[test]
    fn a_third_party_destructive_hint_always_asks() {
        let destructive = hinted("other", &json!({ "destructiveHint": true })).expect("destructive");
        assert!(destructive.strict() && destructive.acts());
        assert_eq!(hinted("other", &json!({ "readOnlyHint": true })), None);
    }
}
