use std::sync::atomic::Ordering;

use serde_json::json;

use super::*;

const CLOUDY: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "cloudy",
  async run(ctx) {
    const done = await ctx.agent("worker", "suba", { cloud: true });
    return { text: done.text };
  },
});
"#;

fn variable(request: &PromptRequest, name: &str) -> Option<String> {
    request.host.environment.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone())
}

async fn first_request(world: &World) -> PromptRequest {
    for _ in 0..500 {
        if let Some(request) = world.engine.requests().into_iter().next() {
            return request;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the agent never started")
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_with_the_cloud_gets_a_profile_whose_credentials_come_from_the_connection_while_it_runs() {
    let mut world = World::new();
    world.cloud = true;
    world.engine.state.hold.store(true, Ordering::SeqCst);
    world.provider.always(
        "cloud.identity",
        json!({ "access_key_id": "AK", "secret_access_key": "SK", "session_token": "ST",
                "region": "sa-east-1", "expires_at": "2100-01-01T00:00:00Z" }),
    );
    world.workflow("cloudy", CLOUDY);
    let kernel = world.start().await;
    let id = kernel.create_item("cloudy", json!({})).expect("item");
    let request = first_request(&world).await;
    assert_eq!(variable(&request, "AWS_PROFILE").as_deref(), Some(brand::COMMAND));
    assert_eq!(variable(&request, "AWS_REGION").as_deref(), Some("sa-east-1"));
    assert_eq!(variable(&request, "AWS_SDK_LOAD_CONFIG").as_deref(), Some("1"));
    assert!(variable(&request, "AWS_ACCESS_KEY_ID").is_none());
    let file = PathBuf::from(variable(&request, "AWS_CONFIG_FILE").expect("config file"));
    let text = std::fs::read_to_string(&file).expect("profile");
    let prefix = format!("{SERVER}{}", crate::CLOUD_CREDENTIALS_PATH);
    let line = text.lines().find(|line| line.starts_with("credential_process")).expect("credential_process");
    assert!(line.contains(&format!(" {} {prefix}", crate::CLOUD_CREDENTIALS_COMMAND)), "{line}");
    let token = line.rsplit(&prefix).next().expect("token").trim().to_owned();
    let answer = kernel.cloud_credentials(&token).await.expect("credentials");
    assert_eq!(
        answer,
        json!({ "Version": 1, "AccessKeyId": "AK", "SecretAccessKey": "SK", "SessionToken": "ST",
                "Expiration": "2100-01-01T00:00:00Z" })
    );
    world.engine.state.hold.store(false, Ordering::SeqCst);
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item), json!({ "text": "feito 1" }));
    assert!(matches!(kernel.cloud_credentials(&token).await, Err(crate::Error::UnknownCloudToken)));
    assert!(!file.exists(), "the profile is gone once the agent ends");
    assert!(world.provider.count("cloud.identity") >= 2);
}

const NAMED: &str = r#"
import { workflow } from "sdk";
export default workflow({
  name: "cloudy",
  async run(ctx) {
    const done = await ctx.agent("worker", "suba", { cloud: "aws-prod" });
    return { text: done.text };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn without_a_cloud_connection_an_agent_that_only_may_use_it_runs_without_one() {
    let world = World::new();
    world.workflow("cloudy", CLOUDY);
    let kernel = world.start().await;
    let id = kernel.create_item("cloudy", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert!(item.impediment.is_none());
    assert!(world.engine.requests()[0].host.environment.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_that_names_a_cloud_connection_waits_for_it() {
    let world = World::new();
    world.workflow("cloudy", NAMED);
    let kernel = world.start().await;
    let id = kernel.create_item("cloudy", json!({})).expect("item");
    let item = until(&kernel, &id, |item| item.impediment.is_some()).await;
    let reason = item.impediment.and_then(|found| found.reason).unwrap_or_default();
    assert!(reason.starts_with(crate::NEEDS_CONNECTION), "{reason}");
    assert!(world.engine.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_token_has_no_credentials() {
    let world = World::new();
    let kernel = world.start().await;
    assert!(matches!(kernel.cloud_credentials("nope").await, Err(crate::Error::UnknownCloudToken)));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_gets_the_account_and_role_chosen_for_its_application() {
    let mut world = World::new();
    world.cloud = true;
    world.provider.always("cloud.identity", json!({ "access_key_id": "AK", "secret_access_key": "SK", "region": "" }));
    let chosen = store::Binding::new("aws", "111/Deploy");
    let bindings = store::Bindings::from([("cloud".to_owned(), chosen)]);
    let application = store::Application { id: "app".into(), name: "Loja".into(), bindings, ..Default::default() };
    world.store().save_application(&application).expect("application");
    world.workflow("cloudy", CLOUDY);
    let kernel = world.start().await;
    let id = kernel.create_item("cloudy", json!({ "application": { "id": "app" } })).expect("item");
    ended(&kernel, &id).await;
    let calls = world.provider.calls.lock().expect("lock").clone();
    let asked: Vec<Value> =
        calls.into_iter().filter(|(name, _, _)| name == "cloud.identity").map(|(_, args, _)| args).collect();
    assert_eq!(asked.first(), Some(&json!({ "binding": "111/Deploy" })), "{asked:?}");
}
