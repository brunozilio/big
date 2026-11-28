use connectors::ActionError;
use serde_json::Value;
use store::AuditRecord;

pub fn event_of(record: &AuditRecord) -> Result<Value, ActionError> {
    let mut event = serde_json::to_value(record).map_err(ActionError::decode)?;
    event["id"] = Value::from(record.hash.clone());
    Ok(event)
}

#[cfg(test)]
mod tests {
    use domain::audit::AuditEvent;
    use serde_json::json;

    use super::*;

    #[test]
    fn each_event_carries_its_hash_as_id() {
        let record = AuditRecord { seq: 7, event: AuditEvent::new(5, "tool.call"), prev: "p".into(), hash: "h".into() };
        let event = event_of(&record).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            (event["id"].clone(), event["seq"].clone(), event["action"].clone()),
            (json!("h"), json!(7), json!("tool.call"))
        );
        assert_eq!(event["schemaVersion"], json!(domain::audit::SCHEMA_VERSION));
    }
}
