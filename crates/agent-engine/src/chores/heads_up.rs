use domain::HeadsUp;
use platform::Stop;

use crate::model::{BlockingTurn, ModelApi, ModelError, TurnRequest};
use crate::transcript::Message;

const TAIL: usize = 24_000;

pub(super) fn look(
    api: &dyn ModelApi,
    template: &TurnRequest,
    history: &[Message],
    stop: &Stop,
) -> Result<Option<HeadsUp>, ModelError> {
    let tail = super::work_text(history, TAIL);
    let request = TurnRequest {
        system: crate::prompt::text("heads-up"),
        messages: vec![Message::user(format!("<conversation>\n{tail}\n</conversation>"))],
        tools: Vec::new(),
        max_tokens: template.max_tokens.min(1500),
        intent: crate::model::Intent::default(),
        ..template.clone()
    };
    Ok(parse(&api.turn_blocking(&request, &mut |_| {}, stop.token())?.message.text()))
}

pub fn parse(answer: &str) -> Option<HeadsUp> {
    let after = |label: &str| {
        answer.lines().find_map(|line| line.trim().strip_prefix(label).map(|rest| rest.trim().trim_matches('`').trim()))
    };
    let learn = after("learn:")?.to_owned();
    if learn.is_empty() || learn.eq_ignore_ascii_case("none") {
        return None;
    }
    let urgent = after("tag:")?.to_ascii_lowercase().starts_with("heads up");
    let body = answer.split_once("explain:")?.1.trim();
    let (title, explain) = match body.strip_prefix("**").and_then(|rest| rest.split_once("**")) {
        Some((title, rest)) => (title.trim().to_owned(), rest.trim().to_owned()),
        None => (String::new(), body.to_owned()),
    };
    (!title.is_empty() && !explain.is_empty()).then_some(HeadsUp { urgent, learn, title, explain })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heads_up_has_its_parts_and_none_is_nothing() {
        let answer = "learn: The cache now spans users, so one user's data can leak to another.\n\
                      tag: Heads up\n\
                      explain:\n\n**Cached answers are shared**\n\nThe main agent keyed the cache by query only.";
        let found = parse(answer).expect("a heads-up");
        assert!(found.urgent);
        assert_eq!(found.title, "Cached answers are shared");
        assert_eq!(found.explain, "The main agent keyed the cache by query only.");
        assert!(found.learn.ends_with("leak to another."));
        assert_eq!(parse("learn: none"), None);
        assert_eq!(parse("learn: x\ntag: You should know\nexplain:\nno title"), None, "out of form");
        let ask = crate::prompt::text("heads-up");
        assert!(!ask.contains("Claude") && ask.contains("learn: none"));
    }

    #[test]
    fn the_long_ask_is_the_cached_system_and_only_the_end_of_the_session_follows() {
        let scripted =
            crate::model::scripted::ScriptedModel::new([crate::model::scripted::ScriptedModel::reply(vec![
                crate::transcript::Block::Text("learn: none".into()),
            ])]);
        let template = TurnRequest {
            model: "small".into(),
            system: "chores".into(),
            messages: Vec::new(),
            tools: Vec::new(),
            effort: domain::Effort::Low,
            max_tokens: 8000,
            intent: crate::model::Intent::ONCE,
        };
        let history = vec![Message::user("x".repeat(TAIL * 2))];
        assert_eq!(look(&scripted, &template, &history, &Stop::new()).expect("looked"), None);
        let asked = &scripted.asked()[0];
        assert_eq!(asked.system, crate::prompt::text("heads-up"));
        assert_eq!(asked.intent, crate::model::Intent::default(), "kept for the next look");
        assert_eq!(asked.messages.len(), 1);
        assert!(asked.messages[0].text().chars().count() < TAIL + 40);
    }
}
