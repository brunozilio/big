use super::*;
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    move |name| map.get(name).cloned()
}

#[test]
fn the_environment_supplies_proxy_bypass_and_certificates() {
    let found = env(&[
        ("https_proxy", "http://proxy:3128"),
        ("HTTP_PROXY", "http://proxy:8080"),
        ("no_proxy", "localhost, *.lan.com,10.0.0.0/8"),
        ("SSL_CERT_FILE", "/etc/ca.pem"),
        ("NODE_EXTRA_CA_CERTS", "/opt/corp.pem"),
    ]);
    let settings = Settings::from(Configured::default(), &found);
    assert_eq!(settings.https_proxy.as_deref(), Some("http://proxy:3128"));
    assert_eq!(settings.http_proxy.as_deref(), Some("http://proxy:8080"));
    assert_eq!(settings.no_proxy, "localhost,.lan.com,10.0.0.0/8");
    assert_eq!(settings.ca_files, vec![PathBuf::from("/etc/ca.pem"), PathBuf::from("/opt/corp.pem")]);
}

#[test]
fn the_configuration_wins_over_the_environment_and_adds_certificates() {
    let found = env(&[("HTTPS_PROXY", "http://env:1"), ("NO_PROXY", "a.com"), ("SSL_CERT_FILE", "/env.pem")]);
    let configured =
        Configured { ca_files: "/corp.pem", https_proxy: "http://own:2", http_proxy: "", no_proxy: "*.lan.com" };
    let settings = Settings::from(configured, &found);
    assert_eq!(settings.https_proxy.as_deref(), Some("http://own:2"));
    assert_eq!(settings.no_proxy, ".lan.com");
    assert_eq!(settings.ca_files, vec![PathBuf::from("/corp.pem"), PathBuf::from("/env.pem")]);
}

#[test]
fn without_anything_set_there_is_no_proxy() {
    let settings = Settings::from(Configured::default(), &env(&[]));
    assert!(!settings.proxied());
    assert_eq!(settings, Settings::default());
}

#[test]
fn an_unreadable_certificate_is_skipped_and_the_client_still_builds() {
    let settings = Settings { ca_files: vec![PathBuf::from("/nowhere/ca.pem")], ..Settings::default() };
    assert!(settings.applied(reqwest::Client::builder()).build().is_ok());
}

#[test]
fn a_bundle_of_certificates_is_loaded() {
    let folder = tempfile::tempdir().expect("folder");
    let path = folder.path().join("ca.pem");
    std::fs::write(&path, PEM).expect("write");
    assert_eq!(certificates(&path).len(), 1);
}

#[test]
fn the_proxy_credentials_never_reach_the_log() {
    assert_eq!(hidden("http://user:secret@proxy:3128"), "http://proxy:3128/");
}

fn listening() -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = format!("http://{}", listener.local_addr().expect("address"));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 512];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let _ = tx.send(String::from_utf8_lossy(&buffer[..read]).into_owned());
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n");
        }
    });
    (address, rx)
}

async fn asked(no_proxy: &str) -> Option<String> {
    let (proxy, seen) = listening();
    let settings = Settings { https_proxy: Some(proxy), no_proxy: no_proxy.to_owned(), ..Settings::default() };
    let client = settings.applied(reqwest::Client::builder()).build().expect("client");
    let _ = client.get("https://sonar.corp.invalid/api").send().await;
    seen.recv_timeout(std::time::Duration::from_millis(300)).ok()
}

#[tokio::test]
async fn an_https_request_tunnels_through_the_proxy() {
    let first = asked("").await.unwrap_or_default();
    assert!(first.starts_with("CONNECT sonar.corp.invalid:443"), "{first}");
}

#[tokio::test]
async fn a_host_in_no_proxy_goes_direct_even_with_a_star() {
    assert_eq!(asked(&bypassed("localhost,*.corp.invalid")).await, None);
}

const PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBMDCB1gIJANnLsVe04kTXMAoGCCqGSM49BAMCMCAxHjAcBgNVBAMMFW5ldHdv
cmstdGVzdHMuaW52YWxpZDAeFw0yNjExMjYxOTA5MjRaFw0zNjExMjMxOTA5MjRa
MCAxHjAcBgNVBAMMFW5ldHdvcmstdGVzdHMuaW52YWxpZDBZMBMGByqGSM49AgEG
CCqGSM49AwEHA0IABMvtbtAtbOjLw8nfmrHXEgPQ57K8vDdsgc+cR9Zx/ERx1Y0j
9nMihU8qmnocSt5tt/KOmResYwOH+S9/AseZoAkwCgYIKoZIzj0EAwIDSQAwRgIh
APpK0kYPE4HDbUbb8kPqqiwYYEfWjtVIgMMVGV+SIr3LAiEA2xoJ6y3ilc9nAw62
LCH9vuf2o4r6p6kBS8pKWanPSB0=
-----END CERTIFICATE-----
";
