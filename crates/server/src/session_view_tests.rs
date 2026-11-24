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
fn a_workflow_plan_approved_hands_off_instead_of_implementing() {
    assert_eq!(handed_off(Decision::Approve(PermissionMode::AcceptEdits)), Decision::Approve(PermissionMode::Plan));
    assert_eq!(handed_off(Decision::DenyWith("menor".into())), Decision::DenyWith("menor".into()));
}

#[test]
fn a_subagent_shows_its_steps_and_the_session_lists_its_tasks_and_background_work() {
    let event = |at_ms: u64, event: EngineEvent| Record::Event { at_ms, event };
    let read = ToolCall {
        id: "r1".into(),
        name: "Read".into(),
        summary: "src/a.rs".into(),
        output: None,
        failed: false,
        input: None,
    };
    let records = vec![
        Record::Prompted { at_ms: 1_000, prompt: "investigue".into() },
        event(
            2_000,
            EngineEvent::AgentSpawned {
                index: 0,
                kind: "explore".into(),
                summary: "procura o bug".into(),
                prompt: String::new(),
            },
        ),
        event(2_100, EngineEvent::AgentEvent { index: 0, event: Box::new(EngineEvent::ToolStarted(read)) }),
        event(2_200, EngineEvent::BackgroundStarted { id: "bash_1".into() }),
        event(
            2_300,
            EngineEvent::Tasks(vec![domain::events::TaskItem {
                id: "1".into(),
                subject: "rodar os testes".into(),
                status: "in_progress".into(),
            }]),
        ),
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

#[test]
fn a_script_shows_its_code_its_steps_and_its_answer_in_one_row() {
    let code = serde_json::json!({ "code": "return 1" });
    let script = ToolCall::new("r1", "REPL", "Ler testes").with_input(code.clone());
    let inner = EngineEvent::ToolStarted(ToolCall::new("repl-2", "Read", "a.rs"));
    let records: Vec<Record> = [
        EngineEvent::ToolStarted(script),
        EngineEvent::AgentSpawned { index: 1, kind: "REPL".into(), summary: "Ler testes".into(), prompt: "return 1".into() },
        EngineEvent::AgentEvent { index: 1, event: Box::new(inner) },
        EngineEvent::ToolResult { id: "r1".into(), output: "1".into(), failed: false },
    ]
    .into_iter()
    .map(|event| Record::Event { at_ms: 1_000, event })
    .collect();

    let events = events(&Shown::of(&records), None);
    let calls = events[0]["calls"].as_array().expect("the calls");

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["input"], code);
    assert_eq!(calls[0]["output"], "1");
    assert_eq!(calls[0]["steps"], serde_json::json!(["Read(a.rs)"]));
}

#[test]
fn a_question_is_never_a_tool_row_and_once_answered_shows_its_answers() {
    let input = json!({ "questions": [{
        "question": "Qual?", "header": "Cor", "multiSelect": false,
        "options": [{ "label": "azul", "description": "" }, { "label": "verde", "description": "" }],
    }] });
    let asked = ToolCall {
        id: "q1".into(),
        name: "AskUserQuestion".into(),
        summary: "Qual?".into(),
        output: None,
        failed: false,
        input: Some(input),
    };
    let mut records = records();
    records.push(Record::Event { at_ms: 4_000, event: EngineEvent::ToolStarted(asked) });
    let waiting = events(&Shown::of(&records), None);
    assert!(waiting.iter().all(|event| event["type"] != "ask"));
    assert_eq!(waiting.iter().filter(|event| event["type"] == "tools").count(), 1);
    let output = "The user answered: \"Qual?\" = \"verde\". Go on with these answers in mind.";
    records.push(Record::Event {
        at_ms: 5_000,
        event: EngineEvent::ToolResult { id: "q1".into(), output: output.into(), failed: false },
    });
    records.push(Record::Event { at_ms: 5_000, event: EngineEvent::ToolFinished { id: "q1".into() } });
    let done = events(&Shown::of(&records), None);
    let ask = done.iter().find(|event| event["type"] == "ask").cloned().unwrap_or_default();
    assert_eq!(ask["answers"], json!(["verde"]));
    assert_eq!(ask["questions"][0]["options"][1]["label"], "verde");
    assert!(done.iter().all(|event| event["calls"].as_array().is_none_or(|calls| calls.iter().all(|c| c["tag"] != "AskUserQuestion"))));
}
