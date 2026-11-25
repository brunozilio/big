use domain::Decision;
use serde_json::json;

use crate::models::{Held, call, no_findings, says};
use crate::stage::{PATIENCE, Stage};

#[test]
fn a_message_steered_during_a_tool_call_is_delivered_with_its_results_and_cannot_be_taken_back_after() {
    let stage = Stage::new();
    let (model, gate) = Held::before_turn(
        1,
        [call("w1", "Bash", json!({"command": "printf 'written\\n' > out.txt"})), says("Written."), no_findings()],
    );
    stage.model(model);
    let mut request = stage.request("Write the file");
    request.policy.reviewer = domain::Reviewer::Human;

    let mut run = stage.start(&request);
    let ask = run.asked();
    run.steer("Also add tests");
    run.answer(&ask, Decision::Allow);
    gate.reached.recv_timeout(PATIENCE).expect("the model is asked again with the tool's results");
    run.unsteer();
    gate.release.send(()).expect("the model waits at the gate");
    stage.end("prompt", run);

    stage.assert(
        r##"
        == prompt: events ==
        SessionStarted <session-1>
        Checkpoint(0)
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        ToolStarted w1 Bash "printf 'written\\n' > out.txt" input="{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        Asked ask-<uuid>-w1 Permission { tool: "Bash", subject: "printf 'written\\n' > out.txt", rememberable: true }
        Answered ask-<uuid>-w1 Allow
        ToolFinished w1
        ToolResult w1 ok "(no output)"
        Steered("Also add tests")
        Unsteered([])
        Text "Written."
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        Completed duration=<ms> cost=none
        == prompt: conversation ==
        user:
          text "Write the file"
          reminder "# Runtime reminder\nAuto mode is on: act without asking for permission. A reviewer checks risky actions before they run and asks the user only when one looks dangerous.…
        assistant:
          tool_use w1 Bash "{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        user:
          tool_result w1 ok "(no output)"
          text "The user sent this message while you were working:\nAlso add tests"
        assistant:
          text "Written."
        == prompt: log appended ==
        prompted "Write the file"
        event SessionStarted
        opened
        event Checkpoint
        message user
        called model=claude-opus-5-5 stop=ToolUse messages=1
        event Usage
        message assistant
        event ToolStarted w1
        event Asked
        event Answered
        event ToolFinished w1
        event ToolResult w1
        event Steered
        message user
        event Unsteered
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=3
        event Usage
        message assistant
        event Completed
        "##,
    );
}

#[test]
fn a_message_steered_while_the_model_answers_without_tools_is_delivered_before_the_run_ends() {
    let stage = Stage::new();
    let (model, gate) = Held::before_turn(0, [says("Done."), says("Tests added too.")]);
    stage.model(model);

    let mut run = stage.start(&stage.request("Write the file"));
    gate.reached.recv_timeout(PATIENCE).expect("the model is asked for its first answer");
    run.steer("Also add tests");
    gate.release.send(()).expect("the model waits at the gate");
    stage.end("prompt", run);

    stage.assert(
        r##"
        == prompt: events ==
        SessionStarted <session-1>
        Checkpoint(0)
        Text "Done."
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        Steered("Also add tests")
        Text "Tests added too."
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        Completed duration=<ms> cost=none
        == prompt: conversation ==
        user:
          text "Write the file"
          reminder "# Runtime reminder\nAuto mode is on: act without asking for permission. A reviewer checks risky actions before they run and asks the user only when one looks dangerous.…
        assistant:
          text "Done."
        user:
          text "The user sent this message while you were working:\nAlso add tests"
        assistant:
          text "Tests added too."
        == prompt: log appended ==
        prompted "Write the file"
        event SessionStarted
        opened
        event Checkpoint
        message user
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=1
        event Usage
        message assistant
        event Steered
        message user
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=3
        event Usage
        message assistant
        event Completed
        "##,
    );
}
