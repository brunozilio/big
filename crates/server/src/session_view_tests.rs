use domain::{AskId, Choice, EngineEvent, Question, ToolCall};

use super::*;

fn records() -> Vec<Record> {
    let tool = ToolCall {
        id: "t1".into(),
        name: "Bash".into(),
        summary: "ls".into(),
        output: None,
        failed: false,
        input: None,
    };
    vec![
        Record::Prompted { at_ms: 1_000, prompt: "faça".into() },
        Record::Event { at_ms: 2_000, event: EngineEvent::ToolStarted(tool) },
        Record::Event { at_ms: 3_000, event: EngineEvent::TextDelta("pronto".into()) },
    ]
}

#[test]
fn a_session_s_records_become_the_page_s_events_with_their_times() {
    let shown = Shown::of(&records());
    let events = events(&shown, None);
    let kinds: Vec<&str> = events.iter().filter_map(|event| event["type"].as_str()).collect();
    assert_eq!(kinds, ["user", "tools", "assistant"]);
    assert_eq!(events[0]["at"], "1970-01-01T00:00:01Z");
    assert_eq!(events[1]["calls"][0]["kind"], "command");
    assert!(events.iter().all(|event| event["id"].is_string()));
    assert_eq!(status(&shown.transcript, false), "running");
}

#[test]
fn a_pending_question_is_shown_last_and_its_answer_becomes_indices() {
    let question = Question {
        header: "Cor".into(),
        text: "Qual?".into(),
        options: vec![
            Choice { label: "azul".into(), description: String::new() },
            Choice { label: "verde".into(), description: String::new() },
        ],
        multi: true,
    };
    let ask = AgentAsk { id: AskId("a1".into()), kind: AskKind::Questions(vec![question]) };
    let shown = Shown::of(&records());
    let listed = events(&shown, Some(&ask));
    let last = listed.last().cloned().unwrap_or_default();
    assert_eq!(last["id"], "a1");
    assert_eq!(last["questions"][0]["multiSelect"], true);
    assert_eq!(status(&shown.transcript, true), "waiting");
    let response = json!({ "kind": "ask", "answers": ["azul, verde"] });
    assert_eq!(decision(&ask.kind, &response), Some(Decision::Answer(vec![vec![0, 1]])));
    assert_eq!(resolved(last, &response)["answers"], json!(["azul, verde"]));
}

#[test]
fn plans_and_permissions_take_the_page_s_decisions() {
    let plan = AskKind::Plan { plan: "p".into() };
    let approve = json!({ "kind": "plan", "decision": "approve" });
    assert_eq!(decision(&plan, &approve), Some(Decision::Approve(PermissionMode::AcceptEdits)));
    let revise = json!({ "kind": "plan", "decision": "revise", "feedback": "menor" });
    assert_eq!(decision(&plan, &revise), Some(Decision::DenyWith("menor".into())));
    let permission = AskKind::Permission { tool: "Bash".into(), subject: "ls".into(), rememberable: true };
    assert_eq!(
        decision(&permission, &json!({ "kind": "permission", "decision": "always" })),
        Some(Decision::AllowAlways)
    );
    assert_eq!(decision(&permission, &json!({ "kind": "nope" })), None);
    assert_eq!(mode_named("Plan"), "plan");
}

#[test]
fn a_subagent_shows_its_steps_and_the_session_lists_its_tasks_and_background_work() {
    let event = |at_ms: u64, event: EngineEvent| Record::Event { at_ms, event };
    let read = ToolCall { id: "r1".into(), name: "Read".into(), summary: "src/a.rs".into(), output: None, failed: false, input: None };
    let records = vec![
        Record::Prompted { at_ms: 1_000, prompt: "investigue".into() },
        event(2_000, EngineEvent::AgentSpawned { index: 0, kind: "explore".into(), summary: "procura o bug".into(), prompt: String::new() }),
        event(2_100, EngineEvent::AgentEvent { index: 0, event: Box::new(EngineEvent::ToolStarted(read)) }),
        event(2_200, EngineEvent::BackgroundStarted { id: "bash_1".into() }),
        event(2_300, EngineEvent::Tasks(vec![domain::events::TaskItem { id: "1".into(), subject: "rodar os testes".into(), status: "in_progress".into() }])),
    ];
    let shown = Shown::of(&records);
    let listed = events(&shown, None);
    let agent = &listed[1]["calls"][0];
    assert_eq!((agent["kind"].as_str(), agent["tag"].as_str()), (Some("subagent"), Some("explore")));
    assert_eq!(agent["steps"].as_array().map(Vec::len), Some(1));
    let work = work(&shown.transcript);
    assert_eq!(work["background"], serde_json::json!(["bash_1"]));
    assert_eq!(work["tasks"][0]["subject"], "rodar os testes");
}
