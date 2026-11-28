mod login;
mod provider;
mod secret;
mod sigv4;
mod sso;

use std::sync::Arc;

use serde_json::Value;

use crate::config::Configuration;
use crate::scripted::bridge::{Script, WRAPPER, wrapper};
use crate::scripted::data_of;
use crate::scripted::host::Local;

const HARNESS: &str = "tests/aws/harness.js";

const BODY: &str = r#"import { sign, signature } from "connectors/aws/sigv4.ts";
import { element, keysOf, signedIdentityRequest } from "connectors/aws/sts.ts";
export const signed = ({ signing, keys }, ctx) => sign(ctx, signing, keys);
export const signatureOf = ({ secret, stamp, region, service, canonical }, ctx) => signature(ctx, secret, stamp, region, service, canonical);
export const identityRequest = ({ token, region, second }, ctx) => signedIdentityRequest(ctx, keysOf(token), region, new Date(second * 1000));
export const elementOf = ({ xml, name }) => element(xml, name);
"#;

pub fn harness() -> Script {
    let mut modules = content::modules_in(None, "connectors/aws/index.ts").unwrap_or_else(|error| panic!("{error}"));
    modules.push((HARNESS.to_owned(), BODY.to_owned()));
    modules.push((WRAPPER.to_owned(), wrapper(HARNESS)));
    let built = Configuration::built();
    Script { modules: Arc::new(modules), data: data_of(built.section("aws"), &built) }
}

pub fn here(name: &str, input: &Value) -> Value {
    harness().invoke_here(name, input, Arc::new(Local)).unwrap_or_else(|error| panic!("{name}: {error}"))
}
