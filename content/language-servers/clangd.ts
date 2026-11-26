const VERSION = "23.1.0";

const asset = (platform: string, sha256: string) => ({
  url: `https://github.com/clangd/clangd/releases/download/${VERSION}/clangd-${platform}-${VERSION}.zip`,
  archive: "zip",
  sha256,
});

const MAC = asset("mac", "1082e6638223b785ca2daf0939f13afcd0bb95c84ee9a4bbaff4745365159253");

export const spec = {
  name: "clangd",
  command: "clangd",
  extensionToLanguage: { ".c": "c", ".h": "c", ".cc": "cpp", ".cpp": "cpp", ".hpp": "cpp" },
  rootMarkers: ["compile_commands.json", "CMakeLists.txt"],
  install: {
    download: {
      program: `clangd_${VERSION}/bin/clangd`,
      platforms: {
        "macos-aarch64": MAC,
        "macos-x86_64": MAC,
        "linux-x86_64": asset("linux", "e53b1a96196095faedb7642cf64964f7fb9ad4a0c1f00dd2c172a3d9dcbafdfd"),
        "windows-x86_64": asset("windows", "23412a240756a162e7b98a282f36aa2a23a88db5ce16a0cbc4fef7253768c810"),
      },
    },
  },
};
