type Node = Record<string, any>;

const children = (node): Node[] => (Array.isArray(node?.content) ? node.content : []);

const textOf = (value) => (typeof value === "string" ? value : undefined);

function marked(text: string, marks: Node[]): string {
  return marks.reduce((inner, mark) => {
    switch (mark?.type) {
      case "code":
        return `\`${inner}\``;
      case "strong":
        return `**${inner}**`;
      case "em":
        return `*${inner}*`;
      case "strike":
        return `~~${inner}~~`;
      case "link":
        return `[${inner}](${textOf(mark?.attrs?.href) ?? ""})`;
      default:
        return inner;
    }
  }, text);
}

function inline(nodes: Node[]): string {
  return nodes
    .map((node) => {
      switch (node?.type) {
        case "text":
          return marked(textOf(node.text) ?? "", Array.isArray(node.marks) ? node.marks : []);
        case "hardBreak":
          return "\n";
        case "mention":
          return textOf(node.attrs?.text) ?? "";
        case "emoji":
          return textOf(node.attrs?.text) ?? textOf(node.attrs?.shortName) ?? "";
        case "inlineCard":
          return textOf(node.attrs?.url) ?? "";
        default:
          return inline(children(node));
      }
    })
    .join("");
}

function list(node: Node, marker: (index: number, item: Node) => string): string {
  return children(node)
    .map((item, index) => {
      const text = item?.type === "taskItem" ? inline(children(item)) : children(item).map(block).join("\n");
      return `${marker(index, item)}${text.trimEnd()}`;
    })
    .join("\n");
}

const INLINE = ["text", "hardBreak", "mention", "emoji", "inlineCard"];

function block(node: Node): string {
  const type = node?.type;
  if (type === "paragraph") return inline(children(node));
  if (type === "heading") {
    const given = node.attrs?.level;
    const level = Number.isInteger(given) && given >= 0 ? Math.min(Math.max(given, 1), 6) : 1;
    return `${"#".repeat(level)} ${inline(children(node))}`;
  }
  if (type === "bulletList") return list(node, () => "- ");
  if (type === "orderedList") return list(node, (index) => `${index + 1}. `);
  if (type === "taskList") return list(node, (_, item) => (item?.attrs?.state === "DONE" ? "- [x] " : "- [ ] "));
  if (type === "codeBlock") return `\`\`\`${textOf(node.attrs?.language) ?? ""}\n${inline(children(node))}\n\`\`\``;
  if (type === "blockquote") return children(node).map((child) => `> ${block(child)}`).join("\n");
  if (type === "rule") return "---";
  if (INLINE.includes(type)) return inline([node]);
  return children(node).map(block).join("\n\n");
}

export function adfToMarkdown(document): string {
  const type = document !== null && typeof document === "object" && !Array.isArray(document) ? textOf(document.type) : undefined;
  if (type === "doc") return children(document).map(block).filter((text) => text !== "").join("\n\n");
  if (type !== undefined) return block(document);
  return textOf(document) ?? "";
}
