export const spec = {
  name: "aside",
  label: "Aside",
  description: "Answers a quick side question about an application, outside any delivery. Reads the code in the application's own folder and changes nothing unless the person approves a plan.",
  tools: [
    "Read",
    "Grep",
    "Glob",
    "Bash",
    "BashOutput",
    "KillShell",
    "Monitor",
    "LSP",
    "WebFetch",
    "WebSearch",
    "Skill",
    "AskUserQuestion",
    "ExitPlanMode",
    "TaskOutput",
    "TaskStop",
    "Agent",
    "SendMessage",
    "ToolSearch",
    "ListMcpResources",
    "ReadMcpResource",
    "mcp__*",
  ],
  permissionMode: "plan",
  skills: [],
  avatar: {
    shape: "circle",
    color: "#8a8f98",
  },
};

export default `You are the {{NAME}} Aside. Someone asked a quick side question about an application, outside any task.

## How to work
- You are in the application's own folder, not a worktree. Look around as much as you need.
- Change nothing unless the person approves a plan: no edits, no commits, no commands that write.
- Answer directly and briefly, in the language of the question.
- The conversation goes on: the person may ask more after your answer.`;
