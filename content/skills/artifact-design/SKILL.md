---
name: artifact-design
description: Design guidance and the page contract for artifacts (reports, summaries, documents and pages published with the Artifact tool). Load before writing any artifact, including a Markdown one - Markdown is never a shortcut past the design pass.
---
## Page contract — read before your first publish

These are the Artifact tool's own rules for the file you publish; the design guidance below builds on them.

**What an artifact is here**: a read-only document. The person opens it from the chat, reads it in a frame, opens it in a new tab and downloads it. There is no database, no comments, no shared state and no way for the page to save anything or talk back to {{NAME}}. Build pages that are complete as documents: a report, a summary, an analysis, a plan, a small interactive explainer or calculator whose state lives only in the page.

**Format**: Always author the page as `.html`. Publish a `.md` file only when the person asks for Markdown. When the person shares a Markdown document or asks to turn one into an artifact, author an HTML page from its content: keep its substance and design the page as you would any other artifact rather than transcribing the Markdown one to one. A `.pdf`, `.csv`, `.json`, `.txt` or image file you produced with another tool can also be published as is, when that file is what the person asked for.

**Skeleton**: A page that does not start with `<!doctype html>` or `<html>` is wrapped at publish time in a skeleton that carries only a charset and a viewport meta plus a small reset: light `color-scheme`, zero body margin with a 14px system font, `img{max-width:100%}` and `[hidden]{display:none!important}` (toggle visibility with `el.hidden`, not `style.display`). So write the page content directly, with your own `<title>` and `<style>` at the top of the file.

**Title**: Put a `<title>` at the top of the HTML; only the first 8KB of the file is scanned for it. It is the artifact's name in the chat, so write a name, not a summary: a short noun phrase, typically two to four words, specific enough to pick this page out among many. When the person already has a name for the thing, use it. Never use a generic category label alone ("Report", "Summary"), and never append an explainer after a dash or colon. The `title` parameter of the Artifact tool overrides the tag; use it only when the file has none. Keep the title stable when you publish the same file again.

**External resources — CDN allowlist (enforced by the page's Content-Security-Policy)**: external scripts load ONLY from https://cdnjs.cloudflare.com (preferred), https://cdn.jsdelivr.net/npm/, https://unpkg.com, https://cdn.tailwindcss.com and https://code.jquery.com; external stylesheets ONLY from https://fonts.googleapis.com, with their font files from https://fonts.gstatic.com (give every face a real fallback stack). Everything else is blocked with no visible error: every other host and, even on those CDNs, anything but a script — images, media, fetch/XHR/WebSocket, a library's runtime fetches. So inline all other CSS and JS and embed images as `data:` URIs. Load a library as `<script src="https://cdnjs.cloudflare.com/ajax/libs/<lib>/<exact version>/<file>">`, the UMD build that defines a global, placed BEFORE the inline `<script>` that uses it. Pin every library URL to an exact version (`react/18.3.1/…` on cdnjs, `react@18.3.1` on jsDelivr or unpkg, `cdn.tailwindcss.com/3.4.17`), never a bare name, a range or a tag. Mermaid diagrams need the library loaded from the allowlist like any other; prefer a hand-drawn inline SVG for a small diagram.

**What the viewer's frame allows**: The page runs in a sandboxed frame with an opaque origin. It cannot open the print dialog, start a download (`<a download>`, `data:`/`blob:` saves), show `alert()`/`confirm()`/`prompt()`, submit a form anywhere, read cookies or reach {{NAME}}'s own server. The viewer around the page already has Download and Open in new tab buttons, and the person prints from the new tab, so never add a Print, Save as PDF or Download button of your own. Forms work as page UI: handle `submit` in script with `preventDefault()`. Copy buttons may work when `navigator.clipboard.writeText` is called inside the click handler; catch its rejection and fall back to selecting the text. Links to other sites open in a new tab: use real `<a href>` links with `target="_blank"`. Other sites cannot be embedded (no iframes of YouTube, maps or forms). Device APIs (camera, microphone, location) are refused; take data as uploaded files instead (file inputs and `FileReader` work).

**Browser storage**: `localStorage` may be unavailable in the sandboxed frame and the accessor can throw, so wrap every read and write in try/catch and render the page correctly without it. Use it, if at all, only for conveniences such as a remembered tab.

**Size**: The published file must be 16MB or smaller, and embedded `data:` URIs count toward that. A page carries its data inline: put a dataset in a `<script type="application/json" id="data">` block and parse it, rather than pasting it into markup by hand.

**Responsive**: The page must also work at phone width (about 400px), and the page body must never scroll horizontally. Keep a side gutter of at least 16px at every width, set once on `body` or one outer wrapper. Use relative units. Let flex and grid rows wrap or stack to one column when narrow, and give any flex or grid child that holds running text, code or a table `min-width: 0`. Put `max-width: 100%` on images and on any `aspect-ratio` box. Only tables, diagrams and code blocks may be wider, each inside its own `overflow-x: auto` container.

**Theme-aware**: The page renders in the person's theme: light or dark, told apart by `prefers-color-scheme`. Define every color as a token, in this shape (token names and count are the design's own):
```css
:root { --bg: …; --fg: …; --accent: … }  /* every token, light values */
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --bg: …; --fg: …; --accent: …; color-scheme: dark } }
:root[data-theme="dark"] { --bg: …; --fg: …; --accent: …; color-scheme: dark }
body { background: var(--bg); color: var(--fg) }
```
Every token gets its first definition on bare `:root`; the dark blocks only redefine tokens. No component rule uses a literal color that reads in one theme only. `body` keeps an explicit token background. A design that deliberately commits to a single look may drop the dark blocks but still sets the background and every color explicitly.

Work the way the design lead at a small, versatile studio would: give each piece a visual identity at the level of treatment the task calls for. Make deliberate choices about palette, typography and layout that are specific to this subject, and avoid templated designs.

## Read the request first

Decide the treatment; designing is a given. A report gets the same craft as a landing page; only the treatment differs.

Most requests here call for a utilitarian treatment: a report, a plan, a memo, a status summary, an analysis. Make it polished, with real typographic hierarchy, considered spacing and a proper palette, but avoid over-designing. Most pages don't need a hero. Keep flourishes tasteful and limited.

Some requests call for an editorial treatment: a landing page, a pitch, a piece they'll present or share widely.

If unsure: a well-composed page is always acceptable; an over-designed visual identity sometimes isn't.

## Fundamentals for every artifact

**Respect what already exists.** Look for an existing design system first: {{INSTRUCTIONS}}, a tokens or theme file, existing component styles. When one exists, apply it; everything below fills gaps and never overrides. Precedence is always the person's own words, then the project's existing system, then your choices.

**Ground it in the subject.** Define one concrete subject, its audience and the page's single job. Include at least one detail only this subject would have (its real units and scales, its document conventions, its terms of art) as content rather than ornament. Use real content throughout: the data you gathered, with its sources, never lorem ipsum and never invented figures presented as real.

**Pair typefaces.** Inter for text and Space Grotesk for headings are good defaults when nothing in the request or the subject calls for other type. Google Fonts is the only font host the CSP allows; link it directly (`<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=...&display=swap">`) and declare a real fallback stack. Keep running text near 65 characters wide. Set a type scale and keep to it. Give headings `text-wrap: balance` and uppercase labels a little letter-spacing.

**Load libraries instead of inlining them.** When the page really needs a library (a charting or highlighting package), load its UMD build from cdnjs, pinned to an exact version, before the inline script that uses its global. Most pages don't need any library; use one only when it does substantial work for the page.

**Choose neutrals deliberately.** A grey with a slight hue bias toward the page's accent looks intentional; a pure mid-grey looks unconsidered.

**Design both themes.** Give the dark theme the same care as the light one: don't simply invert it; keep contrast legible and keep the accent working on both backgrounds. A color defined only inside a media or `[data-theme]` block is the classic unreadable-page bug.

**Use layout for spacing.** Lay out sibling groups with flex or grid and `gap` instead of per-element margins. Use `font-variant-numeric: tabular-nums` wherever digits line up in columns.

**Make repeated elements consistent.** Cards in a row, label/value pairs down a list and badges on sibling items share edges, baselines and inner padding. Let content set a container's height. Make text that can outgrow its track wrap or scroll in its own container; clipped text is a bug.

**Use card styling selectively.** Border, fill, radius and shadow mark an element as a separate object. Apply them by role; the same radius and shadow on every block flattens the hierarchy. Open with big-number tiles only when those figures are the point of the page.

**Draw charts to scale.** Place marks, ticks and labels with one scale, and make every label name a value the chart actually reaches. Color chart text from the theme tokens. Keep marks and labels clear of one another and inside the drawing's bounds; in SVG, leave room in the viewBox for the outermost labels and give every shape an explicit fill.

**Make the page complete at rest.** Everything meant to be read is visible once the page has loaded. A section may animate in, but from a visible resting state, never left at `opacity: 0` waiting for an observer. Size any opener to its content instead of to the viewport.

**Avoid AI-generated design.** AI-generated design clusters around a few looks: warm cream with a serif display and terracotta accent; near-black with a lone acid-green pop; a purple-to-blue gradient hero on white; emoji as section markers; everything centered; `rounded-lg` everywhere; an accent bar on rounded cards. When the person specifies a visual direction, follow it exactly. When nothing is specified, don't spend that freedom on one of these defaults.

**Build cleanly.** Watch for overlapping elements, cascade collisions and silent font fallbacks. Close every non-void element, double-quote attributes, give keyboard focus a visible state and respect `prefers-reduced-motion`. Watch selector specificity so one rule doesn't silently undo another's spacing.

**Writing the copy.** Treat words as design material. Name things by what people recognize instead of how the system is built. Use active voice and short, direct sentences. Avoid mannered devices: asides set off by em-dashes, "not X, but Y" framing, colon-then-reveal sentences, scare quotes and stock phrases. Write in the person's language.

**Structure is information.** Numbering, eyebrows, dividers and labels should encode something true about the content. Numbered markers fit only when the content really is a sequence.

**When the page is a dashboard or a tool.** It is scanned instead of read top to bottom, so the craft shifts to information design. Put the summary before the detail. Encode state in form as well as in numbers (a pill, a chip, a severity stripe). Semantic color (good / warning / critical) is separate from the accent. Interactive elements should look interactive.

## Process

Before writing the page, settle a short design plan and write it into the file itself, as the `:root` block at the top of the page's `<style>`:
- **Color**: the palette as 4-6 named color tokens.
- **Type**: font tokens for 2+ roles.
- **Layout**: the layout concept as a one-line comment above the tokens.

Then build the rest of the page from those tokens. The plan is working material, not part of the answer: one plain sentence on the direction is the most to say about it.

**Write, check once, publish.** Write the file with Write, then publish it with the Artifact tool. A page whose point is logic (dates, money, scoring, parsing) may get one check first: one run of a pure function on a sample input, or one syntax check of its script. Nothing more: the person is waiting for the document. If they report something visibly broken, fix the file and publish the same path again once.

## When the request is editorial

The stance changes here: make opinionated decisions, and take one real aesthetic risk where it serves the work. Review the design plan against the subject before building: if any part reads like the generic default you would produce for any similar page, revise it in the plan itself, then follow the revised plan exactly.

- The opening states the thesis: start with the most characteristic thing in the subject's world.
- Typography sets the personality of the page. Pair the display and body faces deliberately and make the type treatment a memorable part of the design.
- Use motion deliberately: one orchestrated moment is usually more effective than scattered effects, and sometimes none is best.
- Put your boldness in one place; keep everything around it quiet.
