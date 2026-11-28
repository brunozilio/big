use std::collections::BTreeMap;

use serde_json::Value;

use super::fixture::FILE;
use crate::catalog::EventSpec;

fn sample_problems(spec: &EventSpec, sample: &Value) -> Vec<String> {
    let shown = &spec.name;
    let validator = match jsonschema::validator_for(&spec.schema) {
        Ok(validator) => validator,
        Err(error) => return vec![format!("the event {shown} declares a schema that does not read: {error}")],
    };
    let mut found: Vec<String> = validator
        .iter_errors(sample)
        .map(|error| format!("the sample of {shown} does not fit its schema at {}: {error}", error.instance_path))
        .collect();
    for filter in spec.filters.iter().filter(|filter| sample.get(filter.as_str()).is_none()) {
        found.push(format!("the sample of {shown} lacks {filter}, which the event filters by"));
    }
    found
}

pub fn checked(catalog: &[EventSpec], samples: &BTreeMap<String, Value>, polls: bool) -> Vec<String> {
    let mut found = Vec::new();
    if polls && catalog.is_empty() {
        found.push("polls but declares no events(): say which events it emits".to_owned());
    }
    for spec in catalog {
        match samples.get(&spec.name) {
            Some(sample) => found.extend(sample_problems(spec, sample)),
            None => found.push(format!("the event {} has no sample in {FILE} (events)", spec.name)),
        }
    }
    for name in samples.keys().filter(|name| !catalog.iter().any(|spec| &spec.name == *name)) {
        found.push(format!("{FILE} samples the event {name}, which the connector does not declare"));
    }
    found
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn spec() -> EventSpec {
        let declared = json!([{
            "name": "acme.ticket.opened",
            "capability": "tracker",
            "schema": { "type": "object", "required": ["title"], "properties": { "title": { "type": "string" }, "kind": { "type": "string" } } },
            "filters": ["kind"]
        }]);
        crate::catalog::read("acme", &declared).expect("reads").remove(0)
    }

    #[test]
    fn every_event_has_a_sample_that_fits_its_schema() {
        let good = BTreeMap::from([("acme.ticket.opened".to_owned(), json!({ "title": "t", "kind": "bug" }))]);
        assert!(checked(&[spec()], &good, true).is_empty());
        let none = checked(&[spec()], &BTreeMap::new(), true);
        assert!(none[0].contains("has no sample"), "{none:?}");
        let wrong = BTreeMap::from([("acme.ticket.opened".to_owned(), json!({ "title": 3 }))]);
        let found = checked(&[spec()], &wrong, true);
        assert!(found.iter().any(|why| why.contains("does not fit its schema")), "{found:?}");
        assert!(found.iter().any(|why| why.contains("lacks kind")), "{found:?}");
        let stray = BTreeMap::from([("acme.other".to_owned(), json!({}))]);
        assert!(checked(&[], &stray, false)[0].contains("does not declare"));
        assert!(checked(&[], &BTreeMap::new(), true)[0].contains("declares no events()"));
        assert!(checked(&[], &BTreeMap::new(), false).is_empty());
    }
}
