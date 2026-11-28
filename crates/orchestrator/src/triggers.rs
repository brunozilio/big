use connectors::Connectors;
use connectors::catalog::namespace;
use serde_json::{Value, json};

pub const PLATFORM: [&str; 4] =
    [crate::progress::STEP_FINISHED, crate::factory::ESCAPED, crate::factory::RECURRING, crate::schedules::KIND];

pub fn platform() -> Vec<Value> {
    PLATFORM.iter().map(|name| json!({ "name": name, "namespace": namespace(name) })).collect()
}

pub struct Loaded<'a> {
    pub workflow: &'a str,
    pub builtin: bool,
}

pub fn checked(connectors: &Connectors, described: &Value, loaded: &Loaded) -> Result<(), String> {
    for shape in described["workflows"].as_array().into_iter().flatten() {
        for trigger in shape["triggers"].as_array().into_iter().flatten() {
            let kind = trigger["kind"].as_str().unwrap_or_default();
            let workflow = loaded.workflow;
            known(connectors, kind, loaded).map_err(|why| format!("workflows/{workflow}: {why}"))?;
        }
    }
    Ok(())
}

fn spaces(connectors: &Connectors) -> Vec<&str> {
    let declared = connectors.declared().flat_map(|spec| std::iter::once(&spec.name).chain(spec.aliases.iter()));
    let mut found: Vec<&str> =
        PLATFORM.into_iter().map(namespace).chain(declared.map(|name| namespace(name))).collect();
    found.sort_unstable();
    found.dedup();
    found
}

fn known(connectors: &Connectors, kind: &str, loaded: &Loaded) -> Result<(), String> {
    let workflow = loaded.workflow;
    if PLATFORM.contains(&kind) || connectors.declared().any(|spec| spec.name == kind) {
        return Ok(());
    }
    let renamed: Vec<&str> =
        connectors.declared().filter(|spec| spec.answers(kind)).map(|spec| spec.name.as_str()).collect();
    if !renamed.is_empty() && loaded.builtin {
        return Ok(());
    }
    if !renamed.is_empty() {
        let replacement = renamed.join(", ");
        tracing::warn!(workflow, kind, %replacement, "this event name is retired; trigger on the connector's own name");
        return Ok(());
    }
    let spaces = spaces(connectors);
    let space = namespace(kind);
    if !spaces.contains(&space) && domain::command::closest(space, spaces.iter().copied()).is_none() {
        tracing::warn!(workflow, kind, "no loaded connector declares this event; it fires only if one sends it");
        return Ok(());
    }
    let names = PLATFORM.into_iter().chain(connectors.declared().map(|spec| spec.name.as_str()));
    Err(match domain::command::nearest(kind, names) {
        Some(closest) => format!("on('{kind}'): no connector declares this event; the closest is {closest}"),
        None => format!("on('{kind}'): no connector declares this event"),
    })
}
