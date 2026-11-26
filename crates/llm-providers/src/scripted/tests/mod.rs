mod antigravity;
mod chatgpt;
mod chatgpt_client;
mod claude;
mod copilot;
mod copilot_client;
mod grok;
mod grok_client;
mod kiro;
mod kiro_client;
mod kiro_sign_in;
mod quick;
mod llamacpp;
mod smoke;

use domain::ModelProvider;

use crate::Provider;

fn provider(id: &'static str) -> &'static dyn Provider {
    crate::providers::get(ModelProvider::new(id)).unwrap_or_else(|error| panic!("{error}"))
}
