import { textAt } from "sdk/connectors/index.ts";
import { type Api } from "./api.ts";
import { PROJECTS, foundList, lastPage, read, readNamed } from "./operations.ts";

const PAGE = 50;
const MOST_PAGES = 10;

export function shownProject(project) {
  const avatar = textAt(project, "/avatarUrls/48x48");
  return {
    id: textAt(project, "/id"),
    key: textAt(project, "/key"),
    name: textAt(project, "/name"),
    avatar: typeof project?.avatarUrls?.["48x48"] === "string" ? avatar : null,
  };
}

function inputs(page: number, query?: string) {
  const sent: Record<string, unknown> = { startAt: page * PAGE, maxResults: PAGE };
  const trimmed = (query ?? "").trim();
  if (trimmed !== "") sent.query = trimmed;
  return sent;
}

export function visibleProjects(api: Api, query?: string) {
  let [name, answer] = read(api, PROJECTS, inputs(0, query));
  const found: ReturnType<typeof shownProject>[] = [];
  for (let page = 1; page <= MOST_PAGES; page++) {
    const projects = foundList(answer, ["values", "projects"]);
    found.push(...projects.map(shownProject).filter((project) => project.key !== ""));
    if (page === MOST_PAGES || lastPage(answer) || projects.length < PAGE) break;
    answer = readNamed(api, name, inputs(page, query));
  }
  return found;
}
