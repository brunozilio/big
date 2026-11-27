use serde_json::{Value, json};

use super::super::*;
use crate::testing::ScriptedHost;

fn llamacpp() -> ScriptedProvider {
    ScriptedProvider::load(&format!("{FOLDER}llamacpp/{ENTRY}")).unwrap_or_else(|error| panic!("{error}"))
}

fn approved(more: impl IntoIterator<Item = (u16, String)>) -> ScriptedHost {
    let code = json!({ "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://huggingface.co/device",
        "verification_uri_complete": "https://huggingface.co/device?user_code=ABCD-1234", "interval": 5, "expires_in": 900 });
    let granted =
        json!({ "access_token": "hf_oauth_at", "refresh_token": "rt", "expires_in": 3600, "token_type": "bearer" });
    let mut replies = vec![
        (200, code.to_string()),
        (400, json!({ "error": "authorization_pending" }).to_string()),
        (200, granted.to_string()),
        (200, json!({ "name": "bruno" }).to_string()),
    ];
    replies.extend(more);
    ScriptedHost::new(replies, [])
}

fn siblings(names: &[(&str, u64)]) -> String {
    let listed: Vec<Value> = names.iter().map(|(name, size)| json!({ "rfilename": name, "size": size })).collect();
    json!({ "siblings": listed }).to_string()
}

#[test]
fn llamacpp_declares_a_local_chat_wire_and_asks_only_for_a_model() {
    let provider = llamacpp();
    assert_eq!(
        (provider.id(), provider.wire(None), provider.path(None)),
        ("llamacpp-local", "chat", "/chat/completions")
    );
    assert!(provider.serves("anything/at-all.gguf"));
    let fields = provider.fields(domain::Language::default());
    let names: Vec<&str> = fields.as_array().into_iter().flatten().filter_map(|field| field["name"].as_str()).collect();
    assert_eq!(names, ["model"]);
}

#[test]
fn hugging_face_signs_in_with_the_cli_device_code_and_keeps_a_refresh_token() {
    let files = siblings(&[("M.Q4_K_M.gguf", 4)]);
    let mut host = approved([(200, files)]);
    host.fields = json!({ "model": "owner/M-GGUF" });
    let config = llamacpp().sign_in(&host).unwrap_or_else(|error| panic!("{error}"));
    let tokens = config.auth.clone().unwrap_or_else(|| panic!("no tokens"));
    assert_eq!((tokens.access_token.as_str(), tokens.refresh_token.as_str()), ("hf_oauth_at", "rt"));
    assert_eq!(
        host.log.borrow().opened,
        [("https://huggingface.co/device?user_code=ABCD-1234".to_owned(), Some("ABCD-1234".to_owned()))]
    );
    let sent = host.log.borrow().sent.clone();
    assert_eq!(sent[0].0, "https://huggingface.co/oauth/device");
    assert!(sent[0].1.contains("client_id=26be6b09-91c5-47da-9861-d2d2bb7a7e36"), "{}", sent[0].1);
    assert!(sent[2].1.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"), "{}", sent[2].1);
    assert_eq!(host.downloads.borrow()[0]["headers"], json!([["authorization", "Bearer hf_oauth_at"]]));
    let refresh = llamacpp().refresh(&tokens).unwrap_or_else(|| panic!("refresh"));
    assert_eq!(refresh.url, "https://huggingface.co/oauth/token");
    assert!(
        refresh.body.contains("grant_type=refresh_token") && refresh.body.contains("refresh_token=rt"),
        "{}",
        refresh.body
    );
    let lasting = Tokens { refresh_token: String::new(), ..tokens };
    assert!(llamacpp().refresh(&lasting).is_none());
}

#[test]
fn a_denied_code_ends_the_sign_in() {
    let code = json!({ "device_code": "dc", "user_code": "U", "verification_uri": "https://huggingface.co/device" });
    let mut host =
        ScriptedHost::new([(200, code.to_string()), (400, json!({ "error": "access_denied" }).to_string())], []);
    host.fields = json!({ "model": "owner/M-GGUF" });
    let error = llamacpp().sign_in(&host).err().map(|error| error.to_string()).unwrap_or_default();
    assert!(error.contains("denied"), "{error}");
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
    let mut host = approved([(200, base), (200, quantized), (200, files)]);
    host.fields = json!({ "model": "https://huggingface.co/owner/Model-X" });
    llamacpp().sign_in(&host).unwrap_or_else(|error| panic!("{error}"));
    let downloads = host.downloads.borrow();
    assert_eq!(downloads.len(), 1, "{downloads:?}");
    assert_eq!(downloads[0]["name"], "mradermacher/Model-X-GGUF/Model-X.Q4_K_M.gguf");
    assert_eq!(
        downloads[0]["url"],
        "https://huggingface.co/mradermacher/Model-X-GGUF/resolve/main/Model-X.Q4_K_M.gguf"
    );
    assert_eq!(downloads[0]["bytes"], 16);
    let sent: Vec<String> = host.log.borrow().sent.iter().map(|(url, _)| url.clone()).collect();
    assert!(sent[3].ends_with("/api/whoami-v2"), "{sent:?}");
    assert!(sent[5].contains("base_model:quantized:owner/Model-X"), "{sent:?}");
}

#[test]
fn an_asked_quantization_brings_every_part_of_a_split_file() {
    let files = siblings(&[
        ("Q8_0/Big-Q8_0-00002-of-00002.gguf", 5),
        ("Q8_0/Big-Q8_0-00001-of-00002.gguf", 5),
        ("Big-Q4_K_M.gguf", 4),
    ]);
    let mut host = approved([(200, files)]);
    host.fields = json!({ "model": "owner/Big-GGUF:q8_0" });
    llamacpp().sign_in(&host).unwrap_or_else(|error| panic!("{error}"));
    let names: Vec<Value> = host.downloads.borrow().iter().map(|download| download["name"].clone()).collect();
    assert_eq!(
        names,
        [
            json!("owner/Big-GGUF/Q8_0/Big-Q8_0-00001-of-00002.gguf"),
            json!("owner/Big-GGUF/Q8_0/Big-Q8_0-00002-of-00002.gguf")
        ]
    );
}

#[test]
fn a_missing_model_is_told_by_name() {
    let mut host = approved([(404, "{}".into())]);
    host.fields = json!({ "model": "owner/nothing" });
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
    assert_eq!(
        shown,
        [("llamacpp-local:o/r/M.Q4_K_M.gguf", "M.Q4_K_M"), ("llamacpp-local:o/r/B-00001-of-00002.gguf", "B (25%)")]
    );
    assert_eq!(models[0].window, Some(65_536));
}

#[test]
fn a_turn_carries_the_formats_the_context_and_the_effort_switches() {
    let mut body = json!({ "model": "o/r/M.gguf", "messages": [], "temperature": 0.2 });
    llamacpp().shape(None, &mut body, "low", &serde_json::Value::Null).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(body["temperature"], 0.2);
    assert_eq!(body["top_k"], 20);
    assert_eq!(body["local"]["context"], 65_536);
    assert_eq!(body["local"]["template_kwargs"], json!({ "enable_thinking": true, "reasoning_effort": "low" }));
    assert_eq!(body["local"]["formats"][0]["call"]["format"], "xml");
    llamacpp().shape(None, &mut body, "none", &serde_json::Value::Null).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(body["local"]["template_kwargs"], json!({ "enable_thinking": false }));
}
