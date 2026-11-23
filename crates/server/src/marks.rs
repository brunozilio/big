use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use connectors::ProviderDescriptor;
use serde_json::{Value, json};
use store::ConnectionRecord;

pub fn logo(svg: &str) -> String {
    if svg.trim_start().starts_with('<') {
        return format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg.as_bytes()));
    }
    svg.to_owned()
}

pub fn mark(descriptor: &ProviderDescriptor) -> Value {
    json!({ "provider": descriptor.key.as_str(), "name": descriptor.name, "logo": logo(&descriptor.logo) })
}

pub fn mark_of(descriptors: &[ProviderDescriptor], provider: &str) -> Value {
    descriptors
        .iter()
        .find(|descriptor| descriptor.key.as_str() == provider)
        .map_or_else(|| json!({ "provider": provider, "name": provider, "logo": provider }), mark)
}

pub fn serving<'a>(connections: &'a [ConnectionRecord], capability: &str) -> Option<&'a ConnectionRecord> {
    connections.iter().find(|record| record.capabilities.iter().any(|offered| offered == capability))
}

pub fn offering<'a>(descriptors: &'a [ProviderDescriptor], capability: &str) -> Option<&'a ProviderDescriptor> {
    descriptors.iter().find(|descriptor| descriptor.capabilities.iter().any(|offered| offered.as_str() == capability))
}

pub fn reference(descriptors: &[ProviderDescriptor], record: &ConnectionRecord, key: &str, url: Option<&str>) -> Value {
    let mut value = mark_of(descriptors, &record.provider);
    value["connection"] = json!(record.id);
    value["key"] = json!(key);
    value["url"] = json!(url);
    value
}

pub fn iso(millis: i64) -> String {
    jiff::Timestamp::from_millisecond(millis).unwrap_or(jiff::Timestamp::UNIX_EPOCH).to_string()
}

pub fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_svg_becomes_a_data_url_and_a_name_stays_a_name() {
        assert!(logo("<svg/>").starts_with("data:image/svg+xml;base64,"));
        assert_eq!(logo("claude"), "claude");
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
    }
}
