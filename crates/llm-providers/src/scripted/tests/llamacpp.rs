use serde_json::{Value, json};

use super::super::*;
use crate::testing::ScriptedHost;

fn llamacpp() -> ScriptedProvider {
    ScriptedProvider::load(&format!("{FOLDER}llamacpp/{ENTRY}")).unwrap_or_else(|error| panic!("{error}"))
}

fn siblings(names: &[(&str, u64)]) -> String {
    let listed: Vec<Value> = names.iter().map(|(name, size)| json!({ "rfilename": name, "size": size })).collect();
    json!({ "siblings": listed }).to_string()
}

#[test]
fn llamacpp_declares_a_local_chat_wire_and_asks_for_a_model_and_a_token() {
    let provider = llamacpp();
    assert_eq!((provider.id(), provider.wire(None), provider.path(None)), ("llamacpp-local", "chat", "/chat/completions"));
    assert!(provider.serves("anything/at-all.gguf"));
    let fields = provider.fields(domain::Language::default());
    let names: Vec<&str> = fields.as_array().into_iter().flatten().filter_map(|field| field["name"].as_str()).collect();
    assert_eq!(names, ["model", "token"]);
    assert_eq!(fields[1]["optional"], true);
}

#[test]
fn a_repo_without_gguf_downloads_the_preferred_quantization_of_its_namesake() {
    let base = siblings(&[("config.json", 1), ("model-00001-of-00002.safetensors", 9)]);
    let quantized = json!([{ "id": "someone/Other-Finetune-GGUF" }, { "id": "mradermacher/Model-X-GGUF" }]).to_string();
    let files = siblings(&[
        ("Model-X.Q8_0.gguf", 30),
        ("Model-X.Q4_K_M.gguf", 16),
        ("Model-X.mmproj-f16.gguf", 1),
        ("README.md", 1),
    ]);
    let mut host = ScriptedHost::new(
        [(200, json!({ "name": "bruno" }).to_string()), (200, base), (200, quantized), (200, files)],
        [],
    );
    host.fields = json!({ "model": "https://huggingface.co/owner/Model-X", "token": "hf_abc" });
    let config = llamacpp().sign_in(&host).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(config.access_token(), "hf_abc");
    let downloads = host.downloads.borrow();
    assert_eq!(downloads.len(), 1, "{downloads:?}");
    assert_eq!(downloads[0]["name"], "mradermacher/Model-X-GGUF/Model-X.Q4_K_M.gguf");
    assert_eq!(downloads[0]["url"], "https://huggingface.co/mradermacher/Model-X-GGUF/resolve/main/Model-X.Q4_K_M.gguf");
    assert_eq!(downloads[0]["headers"], json!([["authorization", "Bearer hf_abc"]]));
    assert_eq!(downloads[0]["bytes"], 16);
    let sent: Vec<String> = host.log.borrow().sent.iter().map(|(url, _)| url.clone()).collect();
    assert!(sent[0].ends_with("/api/whoami-v2"), "{sent:?}");
    assert!(sent[2].contains("base_model:quantized:owner/Model-X"), "{sent:?}");
}

#[test]
fn an_asked_quantization_brings_every_part_of_a_split_file() {
    let files = siblings(&[
        ("Q8_0/Big-Q8_0-00002-of-00002.gguf", 5),
        ("Q8_0/Big-Q8_0-00001-of-00002.gguf", 5),
        ("Big-Q4_K_M.gguf", 4),
    ]);
    let mut host = ScriptedHost::new([(200, files)], []);
    host.fields = json!({ "model": "owner/Big-GGUF:q8_0", "token": "" });
    let config = llamacpp().sign_in(&host).unwrap_or_else(|error| panic!("{error}"));
    let names: Vec<Value> = host.downloads.borrow().iter().map(|download| download["name"].clone()).collect();
    assert_eq!(names, [json!("owner/Big-GGUF/Q8_0/Big-Q8_0-00001-of-00002.gguf"), json!("owner/Big-GGUF/Q8_0/Big-Q8_0-00002-of-00002.gguf")]);
    assert!(host.downloads.borrow()[0]["headers"].as_array().is_some_and(Vec::is_empty));
    assert!(!config.access_token().is_empty());
}

#[test]
fn a_missing_model_is_told_by_name() {
    let mut host = ScriptedHost::new([(404, "{}".into())], []);
    host.fields = json!({ "model": "owner/nothing", "token": "" });
    let error = llamacpp().sign_in(&host).err().map(|error| error.to_string()).unwrap_or_default();
    assert!(error.contains("owner/nothing"), "{error}");
}

#[test]
fn the_catalog_hides_projectors_and_later_parts_and_shows_progress() {
    let listed = json!({ "data": [
        { "id": "o/r/M.Q4_K_M.gguf", "complete": true },
        { "id": "o/r/M.mmproj-f16.gguf", "complete": true },
        { "id": "o/r/B-00001-of-00002.gguf", "complete": false, "received": 25, "total": 100 },
        { "id": "o/r/B-00002-of-00002.gguf", "complete": false, "received": 0, "total": 100 },
    ] });
    let models = crate::catalog::collect(&llamacpp(), |_| Ok::<_, ProviderError>(listed.clone())).unwrap_or_default();
    let shown: Vec<(&str, &str)> = models.iter().map(|model| (model.id.as_str(), model.label.as_str())).collect();
    assert_eq!(shown, [("llamacpp-local:o/r/M.Q4_K_M.gguf", "M.Q4_K_M"), ("llamacpp-local:o/r/B-00001-of-00002.gguf", "B (25%)")]);
    assert_eq!(models[0].window, Some(65_536));
}

#[test]
fn a_turn_carries_the_formats_the_context_and_the_effort_switches() {
    let mut body = json!({ "model": "o/r/M.gguf", "messages": [], "temperature": 0.2 });
    llamacpp().shape(None, &mut body, "low").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(body["temperature"], 0.2);
    assert_eq!(body["top_k"], 20);
    assert_eq!(body["local"]["context"], 65_536);
    assert_eq!(body["local"]["template_kwargs"], json!({ "enable_thinking": true, "reasoning_effort": "low" }));
    assert_eq!(body["local"]["formats"][0]["call"]["format"], "xml");
    llamacpp().shape(None, &mut body, "none").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(body["local"]["template_kwargs"], json!({ "enable_thinking": false }));
}
