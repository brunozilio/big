type Serve =
  | { host: string; tools?: string[] }
  | { connector: string; actions?: string[]; writes?: boolean };

type Resource = { name: string; description: string; serves: Serve[] };

export const resources: Resource[] = [
  {
    name: "browser",
    description: "Drive the person's browser through the app's extension: tabs, pages, clicks, forms and screenshots.",
    serves: [{ host: "browser" }],
  },
  {
    name: "memory",
    description: "Search, keep, revise and forget what the app remembers about the project named in the project header.",
    serves: [{ host: "memory" }],
  },
  {
    name: "memory-read",
    description: "Search what the app remembers about the project, without changing it.",
    serves: [{ host: "memory", tools: ["memory_search"] }],
  },
  {
    name: "connectors",
    description: "Read through every connection made in the app (trackers, code hosts, clouds, monitoring), without writing.",
    serves: [{ connector: "*", writes: false }],
  },
];
