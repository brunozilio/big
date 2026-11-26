---
name: code-reading
description: How to find your way around code with the right tool for each question - the language server for symbols, Grep for text, Read for a known place - instead of the shell. Preloaded by the agents that explore and plan.
---

# Reading code

Pick the tool by the question, not by habit. The shell is the last resort for reading code, never the first.

## A question about a symbol goes to the LSP tool

When the language servers line of your environment lists the file's language, these answers come from the LSP tool, which follows the language's rules: re-exports, aliases, trait and interface methods, generics. Grep only matches text, so it misses some uses and adds false ones.

| Question | Operation |
| --- | --- |
| Where is this function, type or constant? | `workspaceSymbol` with `query` |
| Where is this defined? | `goToDefinition` at the use |
| Who uses it? How far does a change reach? | `findReferences` |
| Who implements this trait or interface? | `goToImplementation` |
| Who calls this function? What does it call? | `incomingCalls`, `outgoingCalls` |
| What is in this long file? | `documentSymbol`, then Read only the part you need |
| What is its type? | `hover` |
| Is the code already broken before I touch it? | `workspaceDiagnostics` |

`line` and `character` count from 1, as Read shows them; point at the symbol's name. The first question to a project can wait while its server indexes; wait for it instead of falling back to Grep.

When planning a change, size it with `findReferences` and `incomingCalls` on what it touches: the callers are the change's real reach.

## Text goes to Grep, a known place to Read

- Strings, messages, routes, configuration keys, comments, file names: Grep, with `path` set to the folder to search.
- Files by name or pattern: Glob.
- A file you know: Read, with `offset` and `limit` when it is long.
- A language with no server listed: Grep and Read.

## The shell is for what has no tool

- Bash is for git, the build, the tests and running the program. Never `cat`, `head`, `tail`, `grep`, `rg`, `find` or `ls` to read or search: their tools exist.
- Never `cd` before a command: each call starts a fresh shell. Pass absolute paths, or the tool's `path`, to reach another repository or worktree of the task.
- In a REPL script the same tools are functions: `await LSP({ operation: "findReferences", file_path, line, character })`, `await Grep({ pattern, path })`. Reach for `file(path).text()` only when you need the whole file.
