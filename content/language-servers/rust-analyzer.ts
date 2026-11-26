const RELEASE = "https://github.com/rust-lang/rust-analyzer/releases/download/2026-11-09";

const asset = (target: string, archive: "gzip" | "zip", sha256: string) => ({
  url: `${RELEASE}/rust-analyzer-${target}.${archive === "gzip" ? "gz" : "zip"}`,
  archive,
  sha256,
});

export const spec = {
  name: "rust-analyzer",
  command: "rust-analyzer",
  extensionToLanguage: { ".rs": "rust" },
  rootMarkers: ["Cargo.toml"],
  root: "outermost",
  install: {
    download: {
      program: "rust-analyzer",
      platforms: {
        "macos-aarch64": asset("aarch64-apple-darwin", "gzip", "e43a65008f8f4291320f33c0545dd577ad7623a7407b584a27155833d3722dfb"),
        "macos-x86_64": asset("x86_64-apple-darwin", "gzip", "abf410a54e33e9f8b2d37f76dde8884db86a030d6a9cc70c0e3f87a77c084c5c"),
        "linux-aarch64": asset("aarch64-unknown-linux-gnu", "gzip", "813ca3cec78ac6dab99c7bc9d51e63578f6d401b994ecf29cc0dcad86f9b2cd4"),
        "linux-x86_64": asset("x86_64-unknown-linux-gnu", "gzip", "8c039b25cb338b3970c731646677a4c9d4e9dd7a92c9ab9ae459c71467fbf4a9"),
        "windows-aarch64": asset("aarch64-pc-windows-msvc", "zip", "ca0770b76d3881642221d0437f7535e324f22a9499bcd00bbebeb0228617249e"),
        "windows-x86_64": asset("x86_64-pc-windows-msvc", "zip", "973ab11e84955a7f8e3cf01ccd98a33f3ecbd6d4a14bf59bb164aa9e831c6fef"),
      },
    },
  },
};
