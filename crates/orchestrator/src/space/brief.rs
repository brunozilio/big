use domain::PromptRequest;
use serde_json::Value;
use store::Store;

pub fn append(request: &mut PromptRequest, text: String) {
    let system = request.session.append_system.take().unwrap_or_default();
    request.session.append_system = Some(if system.is_empty() { text } else { format!("{system}\n\n{text}") });
}

pub fn brief(store: &Store, input: &Value, request: &mut PromptRequest) {
    let Some(space) = super::live(store, input).flatten() else { return };
    let instructions = space.instructions.trim();
    if !instructions.is_empty() {
        append(request, format!("# Instructions for the work in {}\n{instructions}", space.name));
    }
    for skill in space.skills.iter().map(|skill| skill.trim()).filter(|skill| !skill.is_empty()) {
        if !request.session.skills.iter().any(|known| known == skill) {
            request.session.skills.push(skill.to_owned());
        }
    }
}
