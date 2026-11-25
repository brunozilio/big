use domain::Decision;
use serde_json::json;

use crate::models::{call, no_findings, says};
use crate::stage::Stage;

#[test]
fn a_command_always_allowed_is_remembered_and_not_asked_again() {
    let stage = Stage::new();
    stage.script([
        call("w1", "Bash", json!({"command": "printf 'written\\n' > out.txt"})),
        call("w2", "Bash", json!({"command": "printf 'written\\n' > out.txt"})),
        says("Done."),
        no_findings(),
    ]);
    let mut request = stage.request("Write the file twice");
    request.policy.reviewer = domain::Reviewer::Human;

    let mut run = stage.start(&request);
    let ask = run.asked();
    run.answer(&ask, Decision::AllowAlways);
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
        Answered ask-<uuid>-w1 AllowAlways
        ToolFinished w1
        ToolResult w1 ok "(no output)"
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        ToolStarted w2 Bash "printf 'written\\n' > out.txt" input="{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        ToolFinished w2
        ToolResult w2 ok "(no output)"
        Text "Done."
        Usage model=claude-opus-5-5 input=10 output=5 cost=none
        Completed duration=<ms> cost=none
        == prompt: conversation ==
        user:
          text "Write the file twice"
          reminder "# Runtime reminder\nAuto mode is on: act without asking for permission. A reviewer checks risky actions before they run and asks the user only when one looks dangerous.…
        assistant:
          tool_use w1 Bash "{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        user:
          tool_result w1 ok "(no output)"
        assistant:
          tool_use w2 Bash "{\"command\":\"printf 'written\\\\n' > out.txt\"}"
        user:
          tool_result w2 ok "(no output)"
        assistant:
          text "Done."
        == prompt: log appended ==
        prompted "Write the file twice"
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
        called model=claude-opus-5-5 stop=ToolUse messages=3
        event Usage
        message assistant
        event ToolStarted w2
        event ToolFinished w2
        event ToolResult w2
        message user
        event TextDelta
        called model=claude-opus-5-5 stop=EndTurn messages=5
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
