# llama.cpp

Id `llamacpp-local`. Runs open-weight GGUF models on this machine with
llama.cpp linked into the app (no `llama-server` process): the platform
loads the model, keeps its KV cache between turns and serves an OpenAI-style
`chat/completions` endpoint on the loopback, which this provider points the
`chat` wire at (`ctx.local.endpoint()`).

Connecting asks for a Hugging Face model, written like a GitHub repo
(`owner/repo`, optionally `:QUANT`, e.g.
`huihui-ai/Huihui-Qwen3.8-27B-abliterated:Q8_0`), then signs in the way
`hf auth login` does: the OAuth device flow (RFC 8628) with the hf CLI's public
client (`client_id` in `[llamacpp]`). `POST /oauth/device` gives a code and the
page to approve it, `POST /oauth/token` is polled (`authorization_pending`,
`slow_down`) until it answers with an access token and a refresh token, and
`GET /api/whoami-v2` names the account. The token is renewed with a
`refresh_token` grant on the same endpoint.

The repo's `.gguf` files are listed with `GET /api/models/{repo}?blobs=true`.
A repo that has only safetensors is served from its GGUF quantizations,
found with `filter=base_model:quantized:{repo}` sorted by downloads, those
named after the repo first. The quantization is the one asked for, or the
first of `quants` in `[llamacpp]` that exists; split files (`-00001-of-0000N`)
come with all their parts. Each file is downloaded by the platform into the
models folder (`[local_models] folder`, by default the app's data folder),
resuming a `.part` left by an earlier try. Connecting again with another
model adds it; the catalog (`GET {endpoint}/models`) shows downloaded models,
and the ones still arriving with their percentage.

Each turn carries `local`: the context size (`context`), GPU layers
(`gpu_layers`, all when unset), the template switches for the reasoning
effort (`enable_thinking`, `reasoning_effort`) and the output formats. The
platform renders the chat template stored in the GGUF and splits the answer
with the first format whose `when` appears in that template: `<think>` for
reasoning, and tool calls inside `<tool_call>` either as XML
(`<function=…><parameter=…>`, Qwen 3.5 and later) or JSON
(`{"name", "arguments"}`, Qwen 3, Hermes). Add a format here to teach it
another family. Sampling defaults (`temperature`, `top_p`, `top_k`, `min_p`)
come from `[llamacpp]`.
