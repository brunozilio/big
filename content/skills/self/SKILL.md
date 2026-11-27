---
name: {{COMMAND}}
description: How {{NAME}} itself works and where it keeps things on this computer. Read before answering anything about {{NAME}} - an earlier conversation or message ("what did I say", "find the session where…"), where its sessions, items, memory, logs or settings live, how its tools, skills, agents, connectors and workflows are made and changed - and before searching the disk by hand for its data.
---

# {{NAME}}, from the inside

{{NAME}} is a platform that runs content: the tools, agents, skills, connectors,
providers and workflows are TypeScript and Markdown files the user can change.
The program only gives them primitives (files, processes, HTTP, secrets, the
model) and keeps what happens.

## Finding something from an earlier conversation

Use the **History** tool, never a guess or a hand search through the disk:

1. `search` with two or three distinctive words the user remembers. Every word
   must appear; case and accents do not matter. Narrow with `days` or `cwd`
   only when there are too many hits.
2. `read` the session the hit names, from a little before the turn it found.
3. Quote what was said and give the session (title, folder and date) so the
   user can open it.

Nothing found? Try other words, drop the filters, or list `sessions` of the
period. History covers chats and the steps of work items, in every project,
and the folders kept while {{NAME}} had a former name (shown as `kept as …`).
It sees only this computer: a conversation held on another machine is in that
machine's data folder, so say so instead of insisting.

## Where things live

Ask History for `places`: it names the real folders for this computer, since
they change with the system and the product name. In short:

- **data folder**: the database (session list with titles, folders and dates;
  memories; applications; work items and their events; connections;
  preferences), the `native-sessions` folder with one `<id>.jsonl` per
  conversation, and `logs`. The file name is a hash of the folder and the
  session, so search through History rather than by file name.
- **`~/{{DIR}}`**: the user's own settings, skills, agents, tools and content
  overrides. A file there with the same path as a built-in one replaces it.

Read these places, never write them: the session files and the database are
kept by {{NAME}} while it runs, and a hand edit can corrupt a conversation.
Secrets are encrypted; never read or print them.

## How the parts fit

- **Tools** (`tools/*.ts`): each exports `spec`, `effects`, `summary` and
  `execute(input, ctx)`; `ctx` holds the host primitives (`ctx.fs`, `ctx.proc`,
  `ctx.http`, `ctx.history`, `ctx.sessions`…).
- **Agents** (`agents/*.ts`): a `spec` (tools, permission mode, skills to
  preload) and the prompt. An agent sees only the tools its spec lists.
- **Skills** (`skills/<name>/SKILL.md`): one line of description is always in
  view; the body loads only when the Skill tool is called, like this one.
- **Connectors** (`connectors/<name>/`): how to talk to an outside service
  (trackers, code hosts, clouds, security scanners), declared by the connector,
  never by the program.
- **Workflows** (`workflows/*.ts`): the steps and statuses of a work item; a
  quick side question is a work item of the side-question workflow, answered
  by the aside agent.

To change how {{NAME}} behaves, change or add one of these files; the program
itself does not need to change.
