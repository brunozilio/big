export const spec = {
  name: "Artifact",
  input: {
    "additionalProperties": false,
    "properties": {
      "file_path": {
        "description": "The local file to publish: an .html page (the usual case), or a .md, .pdf, .csv, .txt, .json or image file when that is what the person asked for.",
        "type": "string"
      },
      "title": {
        "description": "The artifact's name, two to four words. Leave it out to use the page's <title>, the Markdown file's first heading, or the file name.",
        "type": "string"
      }
    },
    "required": [
      "file_path"
    ],
    "type": "object"
  },
  description: `Publishes a file you wrote as an artifact: a document the person opens from the chat in {{NAME}}, reads in place and downloads. Use it when the person asks for a report, a summary, a page, a document or anything else they will read or keep rather than see as a chat reply.

Before writing the file, load the \`artifact-design\` skill with the Skill tool and follow its page contract: author an .html page, self-contained (inline CSS and JS, images as data: URIs), with a \`<title>\` and both light and dark themes.

Write the file with Write first (in the working directory, or a scratch folder), then call this tool with its path. The artifact is a snapshot: change the file and publish the same path again to replace it; the person always sees the latest one published from that path in this session. The page is read-only for the person: it has no database, no comments and no way to save changes. After publishing, say in one line what the document is; the chat already shows it, so don't paste its content or a link.`,
};

type Input = { file_path?: unknown; title?: unknown };

export function effects(input: Input | null) {
  return typeof input?.file_path === "string" ? [{ read: input.file_path }] : [];
}

export function summary(input: Input | null) {
  if (typeof input?.title === "string" && input.title.trim() !== "") {
    return input.title.trim();
  }
  return typeof input?.file_path === "string" ? input.file_path : "";
}

function failed(content: string) {
  return { content, isError: true };
}

export function execute(input: Input, ctx) {
  if (typeof input.file_path !== "string" || input.file_path.trim() === "") {
    return failed("Missing required parameter `file_path`.");
  }
  const title = typeof input.title === "string" ? input.title : undefined;
  try {
    const published = ctx.artifacts.publish({ path: input.file_path, title });
    return `Published "${published.title}" (${published.kind}). The person sees it in the chat and opens it from there.\nArtifact id: ${published.id}`;
  } catch (error) {
    return failed(String(error.message ?? error));
  }
}
