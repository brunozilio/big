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
    let events = events(&shown, &[]);
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
    let listed = events(&shown, std::slice::from_ref(&ask));
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
    assert_eq!(decision(&plan, &approve), Some(Decision::Approve(PermissionMode::Auto)));
    let revise = json!({ "kind": "plan", "decision": "revise", "feedback": "menor" });
    assert_eq!(decision(&plan, &revise), Some(Decision::DenyWith("menor".into())));
    let permission = AskKind::permission("Bash", "ls", true);
    assert_eq!(
        decision(&permission, &json!({ "kind": "permission", "decision": "always" })),
        Some(Decision::AllowAlways)
    );
    assert_eq!(decision(&permission, &json!({ "kind": "nope" })), None);
    let listed = AskKind::permission("Bash", "git push", false);
    assert_eq!(decision(&listed, &json!({ "kind": "permission", "decision": "always" })), Some(Decision::Allow));
    assert_eq!(decision(&listed, &json!({ "kind": "permission", "decision": "denied" })), Some(Decision::Deny));
    let asked = ask_event(&AgentAsk { id: domain::AskId("a".into()), kind: listed }, 0);
    assert_eq!(asked["rememberable"], false);
    assert_eq!(mode_named("Plan"), "plan");
}

#[test]
fn a_workflow_plan_approved_hands_off_instead_of_implementing() {
    assert_eq!(handed_off(Decision::Approve(PermissionMode::Auto)), Decision::Approve(PermissionMode::Plan));
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
    let listed = events(&shown, &[]);
    let agent = &listed[1]["calls"][0];
    assert_eq!((agent["kind"].as_str(), agent["tag"].as_str()), (Some("subagent"), Some("explore")));
    assert_eq!(agent["steps"].as_array().map(Vec::len), Some(1));
    let work = work(&shown.transcript, &Jobs::of(&shown.transcript, None));
    assert_eq!(work["background"], serde_json::json!([{ "id": "bash_1", "command": "" }]));
    assert_eq!(work["stoppable"], false);
    assert_eq!(work["tasks"][0]["subject"], "rodar os testes");
}

#[test]
fn a_script_shows_its_code_its_steps_and_its_answer_in_one_row() {
    let code = serde_json::json!({ "code": "return 1" });
    let script = ToolCall::new("r1", "REPL", "Ler testes").with_input(code.clone());
    let inner = EngineEvent::ToolStarted(ToolCall::new("repl-2", "Read", "a.rs"));
    let records: Vec<Record> = [
        EngineEvent::ToolStarted(script),
        EngineEvent::AgentSpawned {
            index: 1,
            kind: "REPL".into(),
            summary: "Ler testes".into(),
            prompt: "return 1".into(),
        },
        EngineEvent::AgentEvent { index: 1, event: Box::new(inner) },
        EngineEvent::ToolResult { id: "r1".into(), output: "1".into(), failed: false },
    ]
    .into_iter()
    .map(|event| Record::Event { at_ms: 1_000, event })
    .collect();

    let events = events(&Shown::of(&records), &[]);
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
    let waiting = events(&Shown::of(&records), &[]);
    assert!(waiting.iter().all(|event| event["type"] != "ask"));
    assert_eq!(waiting.iter().filter(|event| event["type"] == "tools").count(), 1);
    let output = "The user answered: \"Qual?\" = \"verde\". Go on with these answers in mind.";
    records.push(Record::Event {
        at_ms: 5_000,
        event: EngineEvent::ToolResult { id: "q1".into(), output: output.into(), failed: false },
    });
    records.push(Record::Event { at_ms: 5_000, event: EngineEvent::ToolFinished { id: "q1".into() } });
    let done = events(&Shown::of(&records), &[]);
    let ask = done.iter().find(|event| event["type"] == "ask").cloned().unwrap_or_default();
    assert_eq!(ask["answers"], json!(["verde"]));
    assert_eq!(ask["questions"][0]["options"][1]["label"], "verde");
    assert!(done.iter().all(|event| {
        event["calls"].as_array().is_none_or(|calls| calls.iter().all(|c| c["tag"] != "AskUserQuestion"))
    }));
}

#[test]
fn a_conversation_opened_in_plan_mode_outside_any_item_stays_in_plan_mode() {
    let run = domain::trace::RunInfo {
        model: "m".into(),
        provider: "p".into(),
        permission: format!("{:?}", PermissionMode::Plan),
        effort: "medium".into(),
        cwd: "/repo".into(),
        branch: None,
        recent: None,
        status: None,
    };
    let opened = Shown::of(&[Record::Opened { at_ms: 1, run }]);
    assert_eq!(free_mode(None, &opened), Some(PermissionMode::Plan));
    let agent = domain::MainAgentSpec { name: "developer".into(), definition: None };
    assert_eq!(free_mode(Some(&agent), &opened), None, "an item's agent keeps its own mode");
    assert_eq!(free_mode(None, &Shown::of(&records())), None);
}

#[test]
fn an_answer_whose_label_has_commas_is_read_whole() {
    let choice = |label: &str| Choice { label: label.into(), description: String::new() };
    let single = Question {
        header: "Estados".into(),
        text: "Quais estados?".into(),
        options: vec![choice("Pendente, aprovado e cancelado"), choice("Outro")],
        multi: false,
    };
    let multi = Question {
        header: "Rotas".into(),
        text: "Quais rotas?".into(),
        options: vec![choice("GET, POST"), choice("DELETE"), choice("PATCH")],
        multi: true,
    };
    let kind = AskKind::Questions(vec![single, multi]);
    let response =
        serde_json::json!({ "kind": "ask", "answers": ["Pendente, aprovado e cancelado", "GET, POST, PATCH"] });
    assert_eq!(decision(&kind, &response), Some(Decision::Answer(vec![vec![0], vec![0, 2]])));
}

#[test]
fn a_plan_asked_beside_a_question_keeps_its_own_ask_id() {
    let plan = AgentAsk { id: AskId("plan-1".into()), kind: AskKind::Plan { plan: "1. Fazer".into() } };
    let question = Question { header: "Escopo".into(), text: "Qual?".into(), options: Vec::new(), multi: false };
    let asked = AgentAsk { id: AskId("q-1".into()), kind: AskKind::Questions(vec![question]) };
    let mut records = records();
    records.push(Record::Event { at_ms: 4_000, event: EngineEvent::Asked(plan.clone()) });
    records.push(Record::Event { at_ms: 4_000, event: EngineEvent::Asked(asked.clone()) });
    let listed = events(&Shown::of(&records), &[asked, plan]);
    let plans: Vec<&Value> = listed.iter().filter(|event| event["type"] == "plan").collect();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["id"], "plan-1");
    assert_eq!(listed.last().map(|event| event["id"].clone()), Some(json!("q-1")));
}

#[test]
fn a_tool_call_shows_how_long_it_took() {
    let call = ToolCall {
        id: "t1".into(),
        name: "Bash".into(),
        summary: "sleep".into(),
        output: None,
        failed: false,
        input: None,
    };
    let records = vec![
        Record::Prompted { at_ms: 1_000, prompt: "faça".into() },
        Record::Event { at_ms: 1_200, event: EngineEvent::ToolStarted(call) },
        Record::Event {
            at_ms: 4_600,
            event: EngineEvent::ToolResult { id: "t1".into(), output: "ok".into(), failed: false },
        },
        Record::Event { at_ms: 4_700, event: EngineEvent::ToolFinished { id: "t1".into() } },
    ];
    let shown = Shown::of(&records);
    let listed = events(&shown, &[]);
    let calls: Vec<&Value> = listed.iter().filter_map(|event| event["calls"].as_array()).flatten().collect();
    assert_eq!(calls.len(), 1, "{listed:?}");
    assert_eq!(calls[0]["durationMs"], 3_500);
}

#[test]
fn an_answered_question_already_in_the_transcript_is_shown_once_under_the_ask_id() {
    let questions = json!([{ "question": "Qual?", "header": "Cor", "multiSelect": false, "options": [] }]);
    let mut listed = vec![
        json!({ "id": "e1", "type": "user", "at": "1970-01-01T00:00:01Z" }),
        json!({ "id": "e4", "type": "ask", "questions": questions, "answers": ["verde"], "at": "1970-01-01T00:00:04Z" }),
    ];
    let resolved = json!({ "id": "call_9", "type": "ask", "questions": questions, "answers": ["verde"], "at": "1970-01-01T00:00:05Z" });
    merge_answered(&mut listed, vec![resolved]);
    let ids: Vec<&str> = listed.iter().filter_map(|event| event["id"].as_str()).collect();
    assert_eq!(ids, ["e1", "call_9"]);
    let other = json!({ "id": "call_10", "type": "ask", "questions": questions, "answers": ["azul"], "at": "1970-01-01T00:00:06Z" });
    merge_answered(&mut listed, vec![other]);
    assert_eq!(listed.iter().filter(|event| event["type"] == "ask").count(), 2);
}

#[test]
fn a_prompt_another_agent_sent_shows_that_agent_as_its_author() {
    let records = vec![
        Record::Shown { text: "planeje".into(), attached: Vec::new(), from: Some("planner".into()) },
        Record::Prompted { at_ms: 1_000, prompt: "planeje".into() },
    ];
    let events = events(&Shown::of(&records), &[]);
    assert_eq!(events[0]["type"], "user");
    assert_eq!(events[0]["author"]["name"], "planner");
    assert_eq!(events[0]["author"]["you"], Value::Null);
    assert_eq!(events[0]["agent"], "planner");
}

#[test]
fn a_published_artifact_leaves_the_tool_rows_and_becomes_its_own_card() {
    let published = ToolCall {
        id: "a1".into(),
        name: "Artifact".into(),
        summary: "Vendas Q3".into(),
        output: None,
        failed: false,
        input: Some(json!({ "file_path": "/tmp/out/q3.html" })),
    };
    let mut records = records();
    records.push(Record::Event { at_ms: 4_000, event: EngineEvent::ToolStarted(published) });
    let running = events(&Shown::of(&records), &[]);
    assert!(running.iter().all(|event| event["type"] != "artifact"));
    let output =
        "Published \"Vendas Q3\" (page). The person sees it in the chat and opens it from there.\nArtifact id: 0a1b";
    records.push(Record::Event {
        at_ms: 5_000,
        event: EngineEvent::ToolResult { id: "a1".into(), output: output.into(), failed: false },
    });
    records.push(Record::Event { at_ms: 5_000, event: EngineEvent::ToolFinished { id: "a1".into() } });
    let done = events(&Shown::of(&records), &[]);
    let card = done.iter().find(|event| event["type"] == "artifact").cloned().unwrap_or_default();
    assert_eq!(
        (card["artifactId"].clone(), card["title"].clone(), card["kind"].clone(), card["name"].clone()),
        (json!("0a1b"), json!("Vendas Q3"), json!("page"), json!("q3.html"))
    );
    assert!(
        done.iter().all(|event| {
            event["calls"].as_array().is_none_or(|calls| calls.iter().all(|c| c["tag"] != "Artifact"))
        })
    );
}

fn job(id: &str, running: bool) -> app::BackgroundJob {
    app::BackgroundJob { id: id.into(), command: "npm start".into(), running }
}

#[test]
fn a_command_sent_to_the_background_shows_whether_it_still_runs_and_can_be_stopped() {
    let started = "Started in the background as bash_7. Read its output with BashOutput and stop it with KillShell.";
    let records: Vec<Record> = [
        EngineEvent::ToolStarted(ToolCall::new("b1", "Bash", "npm start")),
        EngineEvent::ToolResult { id: "b1".into(), output: started.into(), failed: false },
        EngineEvent::BackgroundStarted { id: "bash_7".into() },
        EngineEvent::ToolStarted(ToolCall::new("b2", "Bash", "ls")),
        EngineEvent::ToolResult {
            id: "b2".into(),
            output: "Started in the background as nothing".into(),
            failed: false,
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(at, event)| Record::Event { at_ms: 1_000 + at as u64, event })
    .collect();
    let shown = Shown::of(&records);
    let mut listed = events(&shown, &[]);
    let live = Jobs::of(&shown.transcript, Some(vec![job("bash_7", true)]));
    live.mark(&mut listed);
    let calls = &listed[0]["calls"];
    assert_eq!(
        (calls[0]["job"].as_str(), &calls[0]["jobRunning"], &calls[0]["jobStoppable"]),
        (Some("bash_7"), &json!(true), &json!(true))
    );
    assert!(calls[1].get("job").is_none(), "a foreground command is no job");
    let work = work(&shown.transcript, &live);
    assert_eq!(work["background"], json!([{ "id": "bash_7", "command": "npm start" }]));
    let mut listed = events(&shown, &[]);
    Jobs::of(&shown.transcript, Some(vec![job("bash_7", false)])).mark(&mut listed);
    assert_eq!(
        (&listed[0]["calls"][0]["jobRunning"], &listed[0]["calls"][0]["jobStoppable"]),
        (&json!(false), &json!(false))
    );
}
