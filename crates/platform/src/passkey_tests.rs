#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, Ed25519KeyPair, KeyPair};

use super::*;

const P256_PREFIX: &str = "3059301306072a8648ce3d020106082a8648ce3d030107034200";
const ED25519_PREFIX: &str = "302a300506032b6570032100";

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex")).collect()
}

fn authenticator_data(flags: u8, counter: u32) -> Vec<u8> {
    let mut data = crate::crypto::sha256(b"example.com");
    data.push(flags);
    data.extend(counter.to_be_bytes());
    data
}

#[test]
fn an_es256_signature_over_the_authenticator_data_and_client_hash_verifies() {
    let random = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &random).expect("pkcs8");
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &random).expect("pair");
    let mut spki = unhex(P256_PREFIX);
    spki.extend(pair.public_key().as_ref());
    let data = authenticator_data(0x05, 3);
    let message = signed_message(&data, br#"{"type":"webauthn.get"}"#);
    let signature = pair.sign(&random, &message).expect("sign");
    assert!(verify(ES256, &spki, &message, signature.as_ref()));
    assert!(!verify(ES256, &spki, b"other", signature.as_ref()));
    assert!(!verify(RS256, &spki, &message, signature.as_ref()));
}

#[test]
fn an_ed25519_signature_verifies() {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("pkcs8");
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("pair");
    let mut spki = unhex(ED25519_PREFIX);
    spki.extend(pair.public_key().as_ref());
    let signature = pair.sign(b"hello");
    assert!(verify(EDDSA, &spki, b"hello", signature.as_ref()));
    assert!(!verify(-999, &spki, b"hello", signature.as_ref()));
}

#[test]
fn the_authenticator_data_gives_its_flags_and_counter() {
    let parsed = Authenticator::parse(&authenticator_data(0x05, 42)).expect("parsed");
    assert!(parsed.present() && parsed.verified());
    assert_eq!(parsed.counter, 42);
    assert_eq!(parsed.rp_id_hash, crate::crypto::sha256(b"example.com"));
    assert_eq!(Authenticator::parse(&[0; 10]), None);
}

#[test]
fn a_broken_key_is_refused() {
    assert_eq!(spki_key(&[]), None);
    assert_eq!(spki_key(&[0x30, 0x05, 0x01]), None);
    assert_eq!(spki_key(&unhex("3003020100")), None);
}
