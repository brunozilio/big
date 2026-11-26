export const spec = {
  name: "pyright",
  command: "pyright-langserver",
  args: ["--stdio"],
  extensionToLanguage: { ".py": "python", ".pyi": "python" },
  rootMarkers: ["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt"],
  install: { npm: ["pyright@1.1.414"] },
};
