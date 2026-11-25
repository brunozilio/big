use agent_engine::agent::Ending;
use agent_engine::intercept::{Interceptor, Interceptors, Level};
use agent_engine::model::ModelError;
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::transcript::{Block, Message, Role};
use domain::{Effort, EngineEvent};

use crate::Rig;

fn intercepting(rig: &Rig, source: &str) -> Interceptors {
    let file = rig.dir.path().join("interceptor.js");
    std::fs::write(&file, source).expect("interceptor");
    let run = file.display().to_string();
    Interceptors { tool: Vec::new(), model: vec![Interceptor::new(run, "", 10, Level::Global)] }
}

fn said(text: &str) -> Result<agent_engine::model::Reply, ModelError> {
    ScriptedModel::reply(vec![Block::Text(text.into())])
}

fn answer(history: &[Message]) -> String {
    history.iter().rev().find(|message| message.role == Role::Assistant).map(Message::text).unwrap_or_default()
}

fn shown_text(rig: &Rig) -> String {
    rig.events()
        .into_iter()
        .filter_map(|event| match event {
            EngineEvent::TextDelta(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn an_interceptor_picks_the_model_and_effort_the_call_goes_out_with() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors = intercepting(
        &rig,
        r#"export default (request, ctx, next) => next({ ...request, model: "other", effort: "low" });"#,
    );
    let model = ScriptedModel::new([said("Feito.")]);
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    let asked = model.asked();
    assert_eq!((asked[0].model.as_str(), asked[0].effort), ("other", Effort::Low));
    assert_eq!(answer(&history), "Feito.");
}

#[test]
fn an_interceptor_asks_again_and_only_the_answer_it_keeps_is_shown() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors = intercepting(
        &rig,
        r#"export default (request, ctx, next) => {
  const first = next();
  return first.text.includes("guess") ? next({ ...request, system: request.system + "\nCite a file." }) : first;
};"#,
    );
    let model = ScriptedModel::new([said("A guess."), said("See src/a.rs.")]);
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    let asked = model.asked();
    assert_eq!(asked.len(), 2);
    assert!(asked[1].system.ends_with("Cite a file."));
    assert_eq!(answer(&history), "See src/a.rs.");
    assert_eq!(shown_text(&rig), "See src/a.rs.");
}

#[test]
fn an_interceptor_rewrites_the_reply_or_answers_without_the_model() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors = intercepting(
        &rig,
        r#"export default (request, ctx, next) => ({ ...next(), text: "The key is [redacted]." });"#,
    );
    let model = ScriptedModel::new([said("The key is sk-123.")]);
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    assert_eq!(answer(&history), "The key is [redacted].");
    assert!(!shown_text(&rig).contains("sk-123"));

    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors = intercepting(&rig, r#"export default () => ({ text: "From the cache." });"#);
    let model = ScriptedModel::new([]);
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    assert!(model.asked().is_empty());
    assert_eq!(answer(&history), "From the cache.");
}

#[test]
fn a_broken_request_from_an_interceptor_fails_the_call_naming_why() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors =
        intercepting(&rig, r#"export default (request, ctx, next) => next({ ...request, history: [] });"#);
    let model = ScriptedModel::new([said("Never.")]);
    let ending = rig.run(&model, &mut vec![Message::user("hi")]);
    assert!(
        matches!(&ending, Err(Ending::Failed(ModelError::Provider(why))) if why.contains("history is empty")),
        "{ending:?}"
    );
    assert!(model.asked().is_empty());
}
