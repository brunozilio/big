export const spec = {
  name: "gopls",
  command: "gopls",
  extensionToLanguage: { ".go": "go" },
  rootMarkers: ["go.mod", "go.work"],
  install: { go: "golang.org/x/tools/gopls@v0.24.0" },
};
