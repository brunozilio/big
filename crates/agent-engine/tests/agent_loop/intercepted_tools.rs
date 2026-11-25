use agent_engine::intercept::{Interceptor, Interceptors, Level};
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::transcript::{Block, Message};
use domain::{Decision, EngineEvent};
use serde_json::json;

use crate::{Rig, answering, tool_use};

fn intercepting(rig: &Rig, matcher: &str, source: &str) -> Interceptors {
    let file = rig.dir.path().join("interceptor.js");
    std::fs::write(&file, source).expect("interceptor");
    let run = file.display().to_string();
    Interceptors { tool: vec![Interceptor::new(run, matcher, 10, Level::Global)], model: Vec::new() }
}

fn results(history: &[Message]) -> Vec<(String, bool)> {
    history
        .iter()
        .flat_map(|message| &message.blocks)
        .filter_map(|block| match block {
            Block::ToolResult { content, is_error, .. } => Some((content.clone(), *is_error)),
            _ => None,
        })
        .collect()
}

fn calling(name: &str, input: serde_json::Value) -> ScriptedModel {
    ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("t", name, input)]),
        ScriptedModel::reply(vec![Block::Text("Done.".into())]),
    ])
}

#[test]
fn an_interceptor_retries_a_failed_tool_with_another_input_and_the_model_reads_the_second_answer() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors = intercepting(
        &rig,
        "Read",
        r#"export default (input, ctx, next) => {
  const first = next(input);
  return first.isError ? next({ ...input, file_path: "notes.txt" }) : first;
};"#,
    );
    let model = calling("Read", json!({"file_path": "missing.txt"}));
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    let results = results(&history);
    assert_eq!(results.len(), 1);
    assert!(results[0].0.contains("alpha") && !results[0].1, "{results:?}");
}

#[test]
fn an_interceptor_answers_for_the_tool_without_running_it_and_other_tools_pass_through() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors =
        intercepting(&rig, "Write", r#"export default () => ({ content: "writes are frozen", isError: true });"#);
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![
            tool_use("w", "Write", json!({"file_path": "new.txt", "content": "x"})),
            tool_use("r", "Read", json!({"file_path": "notes.txt"})),
        ]),
        ScriptedModel::reply(vec![Block::Text("Done.".into())]),
    ]);
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    let results = results(&history);
    assert!(results.iter().any(|(content, failed)| content.contains("writes are frozen") && *failed), "{results:?}");
    assert!(results.iter().any(|(content, failed)| content.contains("alpha") && !failed), "{results:?}");
    assert!(!rig.dir.path().join("new.txt").exists());
}

#[test]
fn an_input_changed_outside_the_tool_s_schema_is_refused_before_the_tool_runs() {
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    rig.workspace.interceptors =
        intercepting(&rig, "Read", r#"export default (input, ctx, next) => next({ path: input.file_path });"#);
    let model = calling("Read", json!({"file_path": "notes.txt"}));
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    let results = results(&history);
    assert!(results[0].1 && results[0].0.starts_with("Interceptor failed:"), "{results:?}");
    assert!(!results[0].0.contains("alpha"));
}

#[cfg(unix)]
#[test]
fn an_input_changed_by_an_interceptor_still_asks_for_permission() {
    let mut rig = Rig::new(domain::Reviewer::Human);
    rig.workspace.interceptors = intercepting(
        &rig,
        "Bash",
        r#"export default (input, ctx, next) => next({ ...input, command: "touch made.txt" });"#,
    );
    let answerer = answering(&rig, Decision::Deny);
    let model = calling("Bash", json!({"command": "echo hi"}));
    let mut history = vec![Message::user("hi")];
    rig.run(&model, &mut history).expect("run");
    answerer.join().expect("answerer");
    let asked = rig.events().into_iter().filter(|event| matches!(event, EngineEvent::Asked(_))).count();
    assert!(asked >= 1, "the changed command was asked about");
    assert!(!rig.dir.path().join("made.txt").exists(), "a denied command does not run");
    assert!(results(&history)[0].1);
}

const MARKED: &str = r#"export default (input, ctx, next) => {
  const answer = next(input);
  return { ...answer, content: `${answer.content}[MARK]` };
};"#;

const GUARDED: &str = r#"export const spec = { name: "guarded", description: "Reads behind a guard", tools: ["Read"] };
export const intercept = {
  tool: { matcher: "Read", run(input: any, ctx: any, next: any) {
    const answer = next(input);
    return { ...answer, content: `${answer.content}[agent:${ctx.agent}]` };
  } },
};
export default "Guard.";"#;

fn read_result(messages: &[Message]) -> String {
    results(messages).into_iter().map(|(content, _)| content).find(|content| content.contains("alpha")).expect("read")
}

#[test]
fn a_ts_agent_s_interceptor_sits_between_the_global_one_and_a_tool_wrapper_file() {
    use agent_engine::agent::Agent;
    use agent_engine::tools::subagent::{self, FoundAgent};
    use agent_engine::tools::{agent_file, subagent_config::AgentConfig};
    agent_engine::providers::install_testing();
    let mut rig = Rig::new(domain::Reviewer::Nobody);
    let root = rig.dir.path().join(brand::DIR);
    for (folder, name, mark) in [("intercept", "trail.js", "[global]"), ("tools", "Read.intercept.js", "[tool]")] {
        std::fs::create_dir_all(root.join(folder)).expect("folder");
        std::fs::write(root.join(folder).join(name), MARKED.replace("[MARK]", mark)).expect("interceptor");
    }
    rig.workspace.interceptors = Interceptors::gathered(None, rig.dir.path(), std::slice::from_ref(&root), &|_| true);
    let file = root.join("agents").join("guarded.ts");
    std::fs::create_dir_all(root.join("agents")).expect("agents");
    std::fs::write(&file, GUARDED).expect("agent");
    let shown = file.display().to_string();
    let text = agent_file::markdown(&shown, GUARDED, &shown).expect("markdown");
    let found = FoundAgent {
        name: "guarded".into(),
        description: "Reads behind a guard".into(),
        instructions: "Guard.".into(),
        config: AgentConfig::parse(&agent_file::header(&text)).expect("config"),
    };
    let read = json!({"file_path": "notes.txt"});
    let model = ScriptedModel::new([
        ScriptedModel::reply(vec![tool_use("main", "Read", read.clone())]),
        ScriptedModel::reply(vec![tool_use(
            "delegate",
            "Agent",
            json!({"description": "Guarded read", "prompt": "read notes", "subagent_type": "guarded"}),
        )]),
        ScriptedModel::reply(vec![tool_use("child", "Read", read)]),
        ScriptedModel::reply(vec![Block::Text("Read.".into())]),
        ScriptedModel::reply(vec![Block::Text("Done.".into())]),
    ]);
    let mut tools = agent_engine::tools::standard();
    subagent::offer(&mut tools, std::slice::from_ref(&found));
    let emit = |_| {};
    let agent = Agent {
        api: &model,
        tools: &tools,
        workspace: &rig.workspace,
        control: &rig.control,
        emit: &emit,
        request: agent_engine::model::TurnRequest { tools: Agent::specs(&tools), ..crate::request() },
        agents: &[found],
        depth: 0,
        turn_limit: 200,
    };
    let mut history = vec![Message::user("hi")];
    agent.run(&mut history).expect("run");
    assert!(read_result(&history).ends_with("[tool][global]"), "{}", read_result(&history));
    let calls = model.asked();
    let child = read_result(&calls[3].messages);
    assert!(child.ends_with("[tool][agent:guarded][global]"), "{child}");
}
