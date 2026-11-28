use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use async_trait::async_trait;
use connectors::{
    AccountOption, ActionError, AuthSpec, Credentials, FakeHttp, Http, Provider, ProviderDescriptor, SecretSource,
    TokenSpec,
};
use database::Target;
use domain::connection::ProviderKey;
use serde_json::Value;

use super::{Reach, opened, password_in, read_with, reader};

const REFERENCE: &str = "vault://databases/app";

#[derive(Debug)]
struct Vault;

#[async_trait]
impl SecretSource for Vault {
    async fn secret(&self, _: &dyn Http, reference: &str) -> Result<String, ActionError> {
        if reference == REFERENCE {
            Ok(r#"{"username":"app","password":"s3cr3t"}"#.to_owned())
        } else {
            Err(ActionError::permanent(format!("no secret {reference}")))
        }
    }
}

#[derive(Debug)]
struct Mock {
    reads: bool,
}

#[async_trait]
impl Provider for Mock {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(if self.reads { "vault" } else { "plain" }),
            name: "Vault".into(),
            logo: String::new(),
            capabilities: Vec::new(),
            auth: AuthSpec::Token(TokenSpec { fields: Vec::new(), form: Default::default() }),
            actions: Vec::new(),
        }
    }

    async fn call(&self, _: &dyn Http, _: &Credentials, _: &str, _: &Value, _: &str) -> Result<Value, ActionError> {
        Ok(Value::Null)
    }

    async fn accounts(&self, _: &dyn Http, _: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        Ok(Vec::new())
    }

    fn secrets(&self) -> Option<&dyn SecretSource> {
        self.reads.then_some(&Vault as &dyn SecretSource)
    }
}

fn providers() -> Vec<Arc<dyn Provider>> {
    vec![Arc::new(Mock { reads: false }), Arc::new(Mock { reads: true })]
}

fn fake() -> Reach {
    Arc::new(|| Ok(Arc::new(FakeHttp::new()) as Arc<dyn Http>))
}

fn framed(stream: &mut std::net::TcpStream, tagged: bool) -> Vec<u8> {
    let mut head = vec![0_u8; if tagged { 5 } else { 4 }];
    stream.read_exact(&mut head).unwrap_or_else(|error| panic!("{error}"));
    let size = head.get(head.len() - 4..).map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
    let mut body = vec![0_u8; size.unwrap_or(4) as usize - 4];
    stream.read_exact(&mut body).unwrap_or_else(|error| panic!("{error}"));
    body
}

fn fake_postgres() -> (u16, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|error| panic!("{error}"));
    let port = listener.local_addr().map(|address| address.port()).unwrap_or_else(|error| panic!("{error}"));
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap_or_else(|error| panic!("{error}"));
        framed(&mut stream, false);
        stream.write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 3]).unwrap_or_else(|error| panic!("{error}"));
        let given = framed(&mut stream, true);
        stream
            .write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 0, b'Z', 0, 0, 0, 5, b'I'])
            .unwrap_or_else(|error| panic!("{error}"));
        String::from_utf8_lossy(given.strip_suffix(&[0]).unwrap_or(&given)).into_owned()
    });
    (port, server)
}

#[test]
fn the_connector_hands_the_password_and_the_database_connects_with_it() {
    let (port, server) = fake_postgres();
    let provider = reader("vault", &providers()).unwrap_or_else(|error| panic!("{error}"));
    let password = read_with(provider, REFERENCE.to_owned(), "password".to_owned(), fake());
    let target = Target { url: format!("postgres://app@127.0.0.1:{port}/app?sslmode=disable"), ca_file: String::new() };
    let database = opened(&target, Some(password));
    assert!(database.is_ok(), "{:?}", database.err());
    assert_eq!(server.join().unwrap_or_default(), "s3cr3t");
}

#[test]
fn a_connector_that_is_missing_or_reads_no_secret_is_refused() {
    let missing = reader("nobody", &providers()).err();
    assert_eq!(missing.as_deref(), Some("no connector is called nobody"));
    let plain = reader("plain", &providers()).err();
    assert_eq!(plain.as_deref(), Some("the connector plain does not read secrets"));
    let provider = reader("vault", &providers()).unwrap_or_else(|error| panic!("{error}"));
    let refused = read_with(provider, "other".to_owned(), "password".to_owned(), fake())();
    assert_eq!(refused, Err("no secret other".to_owned()));
}

#[test]
fn the_password_is_the_declared_field_or_the_whole_secret() {
    assert_eq!(password_in(r#"{"username":"app","password":"s3cr3t"}"#, "password"), "s3cr3t");
    assert_eq!(password_in(r#"{"password":"s3cr3t"}"#, ""), r#"{"password":"s3cr3t"}"#);
    assert_eq!(password_in("plain", "password"), "plain");
}
