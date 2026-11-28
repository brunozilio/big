---
name: standup
description: A standup from the work {{NAME}} keeps - what was done yesterday (or in the period asked) and what is left, item by item. Use when the user asks what they or someone did, what is in progress, what is blocked or what comes next.
---

Answer from the **Worklog** tool only, never from memory or a guess.

1. Call Worklog with `action: "items"`. The period is yesterday unless the
   user names another: pass `days`, or `since` and `until` as ISO dates. For
   a Monday standup, `days: 3` covers the weekend.
2. Asked about someone else? Pass `person` (user id or e-mail). When the
   tool says it is not allowed, tell the user they need the admin grant to
   read other people's work, and stop; do not look for it another way.
3. Write three short sections, one line per item, naming the item by its
   title:
   - **Done**: what each item's `done:` lines say happened in the period
     (steps passed or failed, answers given, items finished or cancelled).
   - **Next**: each item's `next:` lines (the step running, the step waiting
     for a person, the step pending).
   - **Blocked**: items with a `blocked` line, with the reason, and items
     waiting for a person.
4. Leave out an item with nothing done and nothing next. If nothing moved,
   say so in one line.

Arguments: $ARGUMENTS
