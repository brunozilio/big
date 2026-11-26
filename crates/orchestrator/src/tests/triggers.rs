use serde_json::json;
use store::{Application, Binding, Bindings, Repository};

use super::*;

const WAITS: &str = "export default workflow({ name: \"NAME\", triggers: [on(\"KIND\")], async run(ctx) { return await ctx.waitHuman([]); } });";

fn waiting(world: &World, name: &str, kind: &str) {
    let body = WAITS.replace("NAME", name).replace("KIND", kind);
    world.workflow(name, &format!("import {{ workflow, on }} from \"sdk\";\n{body}"));
}

fn checkout(world: &World) -> Application {
    Application {
        id: "app".into(),
        name: "Checkout".into(),
        bindings: Bindings::from([("tracker".to_owned(), Binding::new("c1", "BIG"))]),
        repositories: vec![Repository {
            id: "web".into(),
            bindings: Bindings::from([("code".to_owned(), Binding::new("c1", "o/r"))]),
            directory: world.repo().to_string_lossy().into_owned(),
            ..Repository::default()
        }],
        ..Application::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_trigger_lands_in_the_application_of_the_repository_on_the_pull_request_branches() {
    let world = World::new();
    world.store().save_application(&checkout(&world)).expect("application");
    waiting(&world, "fixes", "code.review.submitted");
    let kernel = world.start().await;
    let data = json!({ "repo": "o/r", "head": "feat/cache", "base": "develop", "title": "Ajustar cache", "verdict": "request_changes" });
    let mut review = super::restart::event("r1", "code.review.submitted", data);
    review.subject = "o/r#12".into();
    assert_eq!(kernel.trigger(&review).expect("trigger"), 1);
    let items = kernel.items().expect("items");
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.input["application"]["id"], "app");
    assert_eq!(kept.input["sources"][0]["repository"], "web");
    let promote = json!([{ "from": "develop", "into": "release", "method": "merge" }, { "from": "release", "into": "main", "method": "merge" }]);
    assert_eq!(kept.input["git"], json!({ "web": { "head": "feat/cache", "base": "develop", "method": "squash", "promote": promote } }));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_issue_trigger_lands_in_the_application_that_tracks_that_connection() {
    let world = World::new();
    world.store().save_application(&checkout(&world)).expect("application");
    waiting(&world, "intake", "tracker.issue.created");
    let kernel = world.start().await;
    let mut created = super::restart::event("t1", "tracker.issue.created", json!({ "title": "Nova" }));
    created.subject = "BIG-9".into();
    assert_eq!(kernel.trigger(&created).expect("trigger"), 1);
    let items = kernel.items().expect("items");
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.input["application"]["id"], "app");
    assert_eq!(kept.input["git"]["web"]["head"], "feature/BIG-9-nova", "the default flow names the branch");
    assert_eq!(kept.input["git"]["web"]["base"], "develop");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_about_something_an_open_activity_already_holds_starts_nothing() {
    let world = World::new();
    waiting(&world, "holder", "nothing.ever");
    waiting(&world, "intake", "tracker.issue.created");
    let kernel = world.start().await;
    kernel.create_item("holder", json!({ "made": { "issue": "BIG-70" } })).expect("item");
    let mut created = super::restart::event("t1", "tracker.issue.created", json!({ "title": "Criada pelo app" }));
    created.subject = "BIG-70".into();
    assert_eq!(kernel.trigger(&created).expect("trigger"), 0);
    created.subject = "BIG-71".into();
    assert_eq!(kernel.trigger(&created).expect("trigger"), 1);
}
