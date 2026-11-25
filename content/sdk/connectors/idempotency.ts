export const marker = (command: string, key: string) => `<!-- ${command}:${key} -->`;

export const marked = (command: string, body: string, key: string) => (key === "" ? body : `${body}\n\n${marker(command, key)}`);

export function strip(command: string, body: string): string {
  const open = `<!-- ${command}:`;
  let kept = "";
  let rest = body;
  for (;;) {
    const start = rest.indexOf(open);
    if (start < 0) break;
    const end = rest.indexOf("-->", start);
    if (end < 0) break;
    kept += rest.slice(0, start);
    rest = rest.slice(end + 3);
  }
  return (kept + rest).trim();
}

export function authored(command: string, body: string, agent: string | undefined): string {
  return agent ? `${body}\n\n<!-- ${command}:agent=${agent} -->` : body;
}

export function author(command: string, body: string): string | null {
  const open = `<!-- ${command}:agent=`;
  const at = body.indexOf(open);
  if (at < 0) return null;
  const start = at + open.length;
  const end = body.indexOf("-->", start);
  if (end < 0) return null;
  const agent = body.slice(start, end).trim();
  return agent === "" ? null : agent;
}

export const carries = (command: string, body: string, key: string) => key !== "" && body.includes(marker(command, key));
