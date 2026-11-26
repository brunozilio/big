export const spec = {
  name: "typescript",
  command: "typescript-language-server",
  args: ["--stdio"],
  extensionToLanguage: {
    ".ts": "typescript",
    ".tsx": "typescriptreact",
    ".js": "javascript",
    ".jsx": "javascriptreact",
    ".mjs": "javascript",
    ".cjs": "javascript",
  },
  rootMarkers: ["tsconfig.json", "jsconfig.json", "package.json"],
  install: { npm: ["typescript-language-server@6.0.3", "typescript@6.0.3"] },
};
