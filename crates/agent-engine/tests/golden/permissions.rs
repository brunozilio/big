use domain::Decision;
use serde_json::json;

use crate::models::{call, no_findings, says};
use crate::stage::Stage;

fn writing(stage: &Stage) -> domain::PromptRequest {
    stage.script([
        call("w1", "Bash", json!({"command": "printf 'written\\n' > out.txt"})),
        says("Done."),
        no_findings(),
    ]);
    let mut request = stage.request("Write the file");
    request.policy.reviewer = domain::Reviewer::Human;
    request
}

#[test]
fn a_flagged_command_waits_for_the_user_and_runs_once_allowed() {
    let stage = Stage::new();
    let request = writing(&stage);

    let mut run = stage.start(&request);
    let ask = run.asked();
    run.answer(&ask, Decision::Allow);
    stage.end("prompt", run);

    stage.disk("out.txt", &stage.project.join("out.txt"));
    stage.disk("settings.local.json", &stage.project.join(brand::DIR).join("settings.local.json"));
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
        Text "Done."
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
        assistant:
          text "Done."
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
        message user
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=3
        event Usage
        message assistant
        event Completed
        == out.txt ==
        written
        == settings.local.json ==
        (absent)
        "##,
    );
}

#[test]
fn a_flagged_command_the_user_denies_never_runs_and_the_model_reads_the_refusal() {
    let stage = Stage::new();
    let request = writing(&stage);

    let mut run = stage.start(&request);
    let ask = run.asked();
    run.answer(&ask, Decision::Deny);
    stage.end("prompt", run);

    stage.disk("out.txt", &stage.project.join("out.txt"));
    stage.assert(
        r##"
        == prompt: events ==
        SessionStarted <session-1>
        Checkpoint(0)
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        ToolStarted w1 Bash "printf 'written\\n' > out.txt" input="{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        Asked ask-<uuid>-w1 Permission { tool: "Bash", subject: "printf 'written\\n' > out.txt", rememberable: true }
        Answered ask-<uuid>-w1 Deny
        Refused { tool: "Bash", reason: "The user did not allow Bash here. Do not retry it; ask them or take another approach." }
        ToolFinished w1
        ToolResult w1 failed "The user did not allow Bash here. Do not retry it; ask them or take another approach."
        Text "Done."
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        Completed duration=<ms> cost=none
        == prompt: conversation ==
        user:
          text "Write the file"
          reminder "# Runtime reminder\nAuto mode is on: act without asking for permission. A reviewer checks risky actions before they run and asks the user only when one looks dangerous.…
        assistant:
          tool_use w1 Bash "{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        user:
          tool_result w1 failed "The user did not allow Bash here. Do not retry it; ask them or take another approach."
        assistant:
          text "Done."
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
        event Refused
        event ToolFinished w1
        event ToolResult w1
        message user
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=3
        event Usage
        message assistant
        event Completed
        == out.txt ==
        (absent)
        "##,
    );
}

#[test]
fn a_flagged_command_denied_with_a_note_carries_the_note_to_the_model() {
    let stage = Stage::new();
    let request = writing(&stage);

    let mut run = stage.start(&request);
    let ask = run.asked();
    run.answer(&ask, Decision::DenyWith("use notes.txt instead".into()));
    stage.end("prompt", run);

    stage.assert(
        r##"
        == prompt: events ==
        SessionStarted <session-1>
        Checkpoint(0)
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        ToolStarted w1 Bash "printf 'written\\n' > out.txt" input="{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        Asked ask-<uuid>-w1 Permission { tool: "Bash", subject: "printf 'written\\n' > out.txt", rememberable: true }
        Answered ask-<uuid>-w1 DenyWith("use notes.txt instead")
        Refused { tool: "Bash", reason: "The user did not allow Bash here and said what to do instead: use notes.txt instead" }
        ToolFinished w1
        ToolResult w1 failed "The user did not allow Bash here and said what to do instead: use notes.txt instead"
        Text "Done."
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        Completed duration=<ms> cost=none
        == prompt: conversation ==
        user:
          text "Write the file"
          reminder "# Runtime reminder\nAuto mode is on: act without asking for permission. A reviewer checks risky actions before they run and asks the user only when one looks dangerous.…
        assistant:
          tool_use w1 Bash "{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        user:
          tool_result w1 failed "The user did not allow Bash here and said what to do instead: use notes.txt instead"
        assistant:
          text "Done."
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
        event Refused
        event ToolFinished w1
        event ToolResult w1
        message user
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=3
        event Usage
        message assistant
        event Completed
        "##,
    );
}
