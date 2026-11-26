use std::io::BufReader;
use std::sync::Arc;

use llm_providers::web_search::Hit;
use serde_json::{Value, json};

use super::super::{Fence, Reach};
use super::call;
use crate::model::http::{Http, Response, ScriptedHttp};
use crate::model::{BoxFuture, CancellationToken, ModelError, Searched};
use crate::tools::{Effect, Network, WebSearcher, Workspace};

fn on(workspace: &Workspace, effects: &[Effect], name: &str, arguments: &Value) -> Result<Value, String> {
    let reach = Reach { workspace, fence: Fence::of(effects), streams: None };
    call(&reach, name, arguments)
}

fn networked(http: Arc<dyn Http>) -> Workspace {
    let mut workspace = Workspace::new("/tmp");
    workspace.services.network = Some(Network(http));
    workspace
}

fn fetched(http: &Arc<ScriptedHttp>, url: &str) -> Result<Value, String> {
    on(&networked(http.clone()), &[Effect::Net { host: None }], "fetch", &json!({ "url": url }))
}

fn asked(http: &ScriptedHttp) -> Vec<String> {
    http.sent().into_iter().map(|sent| sent.url).collect()
}

fn refused(result: Result<Value, String>, why: &str) -> bool {
    result.is_err_and(|error| error.contains(why))
}

#[test]
fn a_page_comes_back_with_its_status_address_type_and_text() {
    let http = Arc::new(ScriptedHttp::with_headers([(
        200,
        vec![("Content-Type".to_owned(), "text/html".to_owned())],
        "<p>Hi</p>".to_owned(),
    )]));
    let page = fetched(&http, "http://serde.rs/").expect("fetched");
    assert_eq!(
        page,
        json!({ "status": 200, "url": "https://serde.rs/", "contentType": "text/html", "body": "<p>Hi</p>" })
    );
    let sent = &http.sent()[0];
    assert!(sent.headers.iter().any(|(key, value)| key == "User-Agent" && value == brand::web::USER_AGENT));
    assert!(sent.headers.iter().all(|(key, _)| key != "Accept"), "no Accept unless asked");

    let http = Arc::new(ScriptedHttp::new([(404, "gone".to_owned())]));
    let missing = on(
        &networked(http.clone()),
        &[Effect::Net { host: None }],
        "fetch",
        &json!({ "url": "https://x.dev/", "accept": "text/markdown" }),
    );
    assert_eq!(missing.expect("answered")["status"], 404);
    assert!(http.sent()[0].headers.iter().any(|(key, value)| key == "Accept" && value == "text/markdown"));
}

#[test]
fn a_redirect_to_another_public_page_is_followed_one_hop_at_a_time() {
    let http = Arc::new(ScriptedHttp::with_headers([
        ScriptedHttp::redirect("/docs/"),
        ScriptedHttp::redirect("http://other.dev/page"),
        ScriptedHttp::page("# Moved here"),
    ]));
    let page = fetched(&http, "https://docs.dev/old").expect("fetched");
    assert_eq!((page["url"].as_str(), page["body"].as_str()), (Some("https://other.dev/page"), Some("# Moved here")));
    assert_eq!(asked(&http), ["https://docs.dev/old", "https://docs.dev/docs/", "https://other.dev/page"]);
}

#[test]
fn a_redirect_to_the_cloud_metadata_address_is_refused_before_it_is_asked() {
    let http = Arc::new(ScriptedHttp::with_headers([
        ScriptedHttp::redirect("http://169.254.169.254/latest/meta-data/"),
        ScriptedHttp::page("secret"),
    ]));
    assert!(refused(fetched(&http, "https://evil.dev/"), "private network"));
    assert_eq!(asked(&http), ["https://evil.dev/"], "the private address is never asked");
}

#[test]
fn a_redirect_to_this_computer_is_refused_before_it_is_asked() {
    for target in ["https://localhost:8080/admin", "https://127.0.0.1/", "https://[::1]/", "//10.0.0.2/"] {
        let http = Arc::new(ScriptedHttp::with_headers([ScriptedHttp::redirect(target), ScriptedHttp::page("x")]));
        assert!(refused(fetched(&http, "https://evil.dev/"), "private network"), "{target}");
        assert_eq!(asked(&http).len(), 1, "{target}");
    }
}

#[test]
fn a_redirect_to_something_that_is_not_the_web_is_refused() {
    let http = Arc::new(ScriptedHttp::with_headers([ScriptedHttp::redirect("file:///etc/passwd")]));
    assert!(refused(fetched(&http, "https://evil.dev/"), "Not a web address"));
}

#[test]
fn a_public_name_that_resolves_to_this_computer_is_never_asked() {
    let loopback = "127.0.0.1".parse().expect("address");
    let http = Arc::new(ScriptedHttp::with_headers([ScriptedHttp::page("x")]).resolving("rebound.dev", loopback));
    let out = fetched(&http, "https://rebound.dev/");
    assert!(refused(out.clone(), "private network") && refused(out, "127.0.0.1"), "says what it resolved to");
    assert!(asked(&http).is_empty());

    let metadata = "169.254.169.254".parse().expect("address");
    let http = Arc::new(
        ScriptedHttp::with_headers([ScriptedHttp::redirect("https://rebound.dev/"), ScriptedHttp::page("x")])
            .resolving("rebound.dev", metadata),
    );
    assert!(refused(fetched(&http, "https://fine.dev/"), "private network"), "after a redirect too");
    assert_eq!(asked(&http), ["https://fine.dev/"]);
}

#[test]
fn five_redirects_are_followed_and_a_sixth_is_not() {
    let hops = |count: usize| {
        let redirects = (0..count).map(|at| ScriptedHttp::redirect(&format!("https://hop.dev/{at}")));
        Arc::new(ScriptedHttp::with_headers(redirects.chain([ScriptedHttp::page("# End")]).collect::<Vec<_>>()))
    };
    assert_eq!(fetched(&hops(5), "https://hop.dev/start").expect("fetched")["body"], "# End");
    let http = hops(6);
    assert!(refused(fetched(&http, "https://hop.dev/start"), "redirect too many"));
    assert_eq!(asked(&http).len(), 6, "the seventh address is never asked");
}

#[derive(Debug)]
struct Endless;

impl Http for Endless {
    fn get(&self, _url: &str, _headers: &[(String, String)]) -> Result<Response, String> {
        let body = crate::model::http::Body::reader(BufReader::new(std::io::repeat(b'a')));
        Ok(Response { status: 200, headers: vec![("content-type".into(), "text/plain".into())], body })
    }

    fn addresses(&self, _host: &str) -> Result<Vec<std::net::IpAddr>, String> {
        Ok(vec!["93.184.216.34".parse().map_err(|_| "address")?])
    }

    fn post(&self, _url: &str, _headers: &[(String, String)], _body: &str) -> Result<Response, String> {
        Err("not used".into())
    }
}

#[test]
fn a_body_larger_than_the_cap_stops_being_read_at_the_cap() {
    let page = on(
        &networked(Arc::new(Endless)),
        &[Effect::Net { host: None }],
        "fetch",
        &json!({ "url": "https://big.dev/" }),
    )
    .expect("fetched");
    assert_eq!(page["body"].as_str().map(str::len), Some(5 << 20));
}

#[test]
fn a_fetch_reaches_only_the_host_its_call_declared_and_needs_the_web() {
    let http = Arc::new(ScriptedHttp::new([(200, "x".to_owned())]));
    let workspace = networked(http.clone());
    let declared = [Effect::Net { host: Some("docs.rs".into()) }];
    assert!(refused(on(&workspace, &declared, "fetch", &json!({ "url": "https://evil.dev/" })), "declared"));
    assert!(http.sent().is_empty());
    assert!(on(&workspace, &declared, "fetch", &json!({ "url": "http://docs.rs/" })).is_ok());

    let offline = Workspace::new("/tmp");
    let any = [Effect::Net { host: None }];
    assert!(refused(on(&offline, &any, "fetch", &json!({ "url": "https://docs.rs/" })), "not available to this run"));
    assert!(refused(on(&offline, &any, "fetch", &json!({ "url": "https://localhost/" })), "private network"));
    assert!(refused(on(&offline, &any, "teleport", &json!({})), "is not a function"));
}

#[test]
fn the_reading_flags_come_from_the_policy_and_the_config() {
    let mut workspace = Workspace::new("/tmp");
    workspace.policy.web = domain::web_reading::WebReading::none();
    let reading = on(&workspace, &[], "reading", &Value::Null).expect("reading");
    assert_eq!(reading["links"], false);
    assert_eq!(reading["configured"]["links"], domain::web_reading::WebReading::configured().links);
}

struct Account(Result<Vec<Hit>, ModelError>);

impl crate::model::Searcher for Account {
    fn search<'a>(&'a self, _query: &'a str, _cancel: &'a CancellationToken) -> BoxFuture<'a, Searched> {
        Box::pin(std::future::ready(Some(self.0.clone())))
    }
}

fn searching(account: Option<Result<Vec<Hit>, ModelError>>, page: Option<&str>) -> (Workspace, Arc<ScriptedHttp>) {
    let http = Arc::new(ScriptedHttp::new(page.map(|page| (200, page.to_owned()))));
    let mut workspace = networked(http.clone());
    workspace.services.search = account.map(|found| WebSearcher(Arc::new(Account(found))));
    (workspace, http)
}

fn searched(workspace: &Workspace, query: &str) -> Result<Value, String> {
    on(workspace, &[Effect::Net { host: None }], "search", &json!({ "query": query }))
}

#[test]
fn the_account_s_own_search_answers_first_and_duckduckgo_is_the_fallback() {
    let hit = Hit { title: "Rust".into(), url: "https://rust-lang.org/".into(), snippet: String::new() };
    let (workspace, http) = searching(Some(Ok(vec![hit])), None);
    assert_eq!(searched(&workspace, "rust").expect("hits")["hits"][0]["url"], "https://rust-lang.org/");
    assert!(http.sent().is_empty());

    let (workspace, http) = searching(Some(Ok(Vec::new())), Some("<html>results</html>"));
    assert_eq!(searched(&workspace, "rust 1.90").expect("page"), json!({ "page": "<html>results</html>" }));
    assert_eq!(asked(&http), ["https://html.duckduckgo.com/html/?q=rust+1.90"]);

    let (workspace, _) = searching(Some(Err(ModelError::Unauthorized("HTTP 403".into()))), None);
    let both = searched(&workspace, "rust");
    assert!(refused(both.clone(), "The web search failed: ") && refused(both, "HTTP 403"));

    let (mut workspace, _) = searching(None, None);
    workspace.services.network = None;
    assert!(refused(searched(&workspace, "rust"), "not available to this run"));
}

fn streamed(workspace: &Workspace, effects: &[Effect], name: &str, arguments: &Value) -> Result<Value, String> {
    let streams = crate::model::script_http::Streams::default();
    let reach = Reach { workspace, fence: Fence::of(effects), streams: Some(&streams) };
    call(&reach, name, arguments)
}

#[test]
fn a_script_request_and_stream_go_through_the_fence_and_the_network() {
    let any = [Effect::Net { host: None }];
    let http = Arc::new(ScriptedHttp::new([(200, "ok".to_owned()), (200, "data: one\n\n".to_owned())]));
    let workspace = networked(http.clone());
    let answer = streamed(&workspace, &any, "request", &json!({ "url": "http://api.dev/x", "body": "{}" }));
    assert_eq!(answer.map(|answer| answer["body"].clone()), Ok(json!("ok")));
    assert_eq!(asked(&http), ["https://api.dev/x"]);

    let streams = crate::model::script_http::Streams::default();
    let reach = Reach { workspace: &workspace, fence: Fence::of(&any), streams: Some(&streams) };
    let open = call(&reach, "stream.open", &json!({ "url": "https://api.dev/s", "body": "{}" })).unwrap_or_default();
    assert_eq!(call(&reach, "stream.next", &open["id"]), Ok(json!({ "event": "", "data": "one" })));
    assert_eq!(call(&reach, "stream.next", &open["id"]), Ok(Value::Null));

    let declared = [Effect::Net { host: Some("docs.rs".into()) }];
    assert!(refused(streamed(&workspace, &declared, "request", &json!({ "url": "https://evil.dev/" })), "declared"));
    assert!(refused(on(&workspace, &any, "request", &json!({ "url": "https://api.dev/" })), "not available here"));
    let offline = Workspace::new("/tmp");
    assert!(refused(streamed(&offline, &any, "request", &json!({ "url": "https://api.dev/" })), "not available"));
}

#[test]
fn crypto_is_a_host_space() {
    let workspace = Workspace::new("/tmp");
    let digest = super::super::call(
        &Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None },
        "crypto.sha256",
        &json!({ "data": "" }),
    );
    assert_eq!(digest, Ok(json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")));
}
