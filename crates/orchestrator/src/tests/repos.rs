use serde_json::json;

use super::*;

const MULTI: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "multi",
  async run(ctx, item) {
    const done = await ctx.agent("worker", "faça");
    const repos = item.application.repositories;
    const listed = await ctx.all(repos.map((repo) => (branch) => branch.exec("ls", { repository: repo.id })));
    return {
      changed: done.repositories.map((repo) => [repo.id, repo.changed]),
      ls: listed.map((ran) => ran.stdout.trim()),
    };
  },
});
"#;

fn git_repo(path: &Path, file: &str) {
    std::fs::create_dir_all(path).expect("dir");
    std::fs::write(path.join(file), "oi").expect("write");
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["add", "-A"],
        vec!["commit", "-qm", "início"],
        vec!["branch", "develop"],
    ] {
        let done = platform::git::command(path).args(["-c", "user.email=a@b", "-c", "user.name=a"]).args(args).output();
        assert!(done.is_ok_and(|done| done.status.success()));
    }
}

fn application(first: &Path, second: &Path) -> Value {
    json!({
        "id": "app", "name": "Loja",
        "repositories": [
            { "id": "web", "directory": first.to_string_lossy(), "bindings": { "code": { "connection": "c1", "value": "acme/web" } },
              "run": "bun dev", "verify": ["bun test"] },
            { "id": "api", "directory": second.to_string_lossy(), "verify": ["cargo test"] },
        ],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_of_an_application_gets_a_worktree_per_repository_and_its_own_workflows() {
    let mut world = World::new();
    world.init_git();
    let first = world.dir.path().join("web");
    let second = world.dir.path().join("api");
    git_repo(&second, "API.txt");
    let file = first.join(brand::DIR).join("workflows/multi.ts");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    std::fs::write(&file, MULTI).expect("write");
    let agent = first.join(brand::DIR).join("agents/worker.md");
    std::fs::create_dir_all(agent.parent().expect("parent")).expect("dir");
    std::fs::write(&agent, WORKER).expect("write");
    git_repo(&first, "WEB.txt");
    *world.engine.state.write.lock().expect("lock") = Some("feito.txt".into());
    let kernel = world.start().await;

    assert!(kernel.create_item("multi", json!({ "title": "x" })).is_err());
    let input = json!({ "title": "x", "application": application(&first, &second) });
    let id = kernel.create_item("multi", input).expect("item");
    let item = ended(&kernel, &id).await;
    let done = output(&item);
    assert_eq!(done["changed"], json!([["web", true], ["api", false]]));
    let ls = done["ls"].as_array().expect("ls");
    assert!(ls[0].as_str().is_some_and(|text| text.contains("WEB.txt") && text.contains("feito.txt")), "{done}");
    assert!(ls[1].as_str().is_some_and(|text| text.contains("API.txt") && !text.contains("WEB.txt")), "{done}");

    let request = &world.engine.requests()[0];
    assert!(request.session.cwd.to_string_lossy().ends_with("--web"));
    assert_eq!(request.session.add_dirs.len(), 1);
    assert!(request.session.add_dirs[0].to_string_lossy().ends_with("--api"));
    let system = request.session.append_system.clone().unwrap_or_default();
    assert!(system.contains("`bun dev`") && system.contains("`cargo test`"), "{system}");
    drop(kernel);

    let again = world.start().await;
    let replayed = again.item(&id).expect("item").expect("kept");
    assert_eq!(output(&replayed), done);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_repository_is_consulted_in_a_detached_copy_the_agent_cannot_write() {
    let mut world = World::new();
    world.init_git();
    let first = world.dir.path().join("web");
    let second = world.dir.path().join("cfg");
    let file = second.join(brand::DIR).join("workflows/multi.ts");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    std::fs::write(&file, MULTI).expect("write");
    let agent = second.join(brand::DIR).join("agents/worker.md");
    std::fs::create_dir_all(agent.parent().expect("parent")).expect("dir");
    std::fs::write(&agent, WORKER).expect("write");
    git_repo(&second, "CFG.txt");
    git_repo(&first, "WEB.txt");
    *world.engine.state.write.lock().expect("lock") = Some("feito.txt".into());
    world.engine.state.hold.store(true, Ordering::SeqCst);
    let kernel = world.start().await;

    let mut application = application(&first, &second);
    application["repositories"][1]["readOnly"] = json!(true);
    let repositories = application["repositories"].as_array_mut().expect("listed");
    repositories.reverse();
    let id = kernel.create_item("multi", json!({ "title": "x", "application": application })).expect("item");
    let request = eventually(|| world.engine.requests().into_iter().next()).await.expect("the agent started");
    consulted_while_the_agent_works(&request, &second);
    world.engine.state.hold.store(false, Ordering::SeqCst);
    let item = ended(&kernel, &id).await;
    let done = output(&item);
    assert_eq!(done["changed"], json!([["web", true], ["api", false]]));
    assert!(done["ls"][0].as_str().is_some_and(|text| text.contains("CFG.txt")), "{done}");
}

fn consulted_while_the_agent_works(request: &PromptRequest, second: &Path) {
    assert!(request.session.cwd.to_string_lossy().ends_with("--web"));
    assert!(request.session.add_dirs.is_empty(), "{:?}", request.session.add_dirs);
    let copy = request.session.read_dirs.first().expect("consulted").clone();
    assert!(copy.to_string_lossy().ends_with("--api") && copy != second, "{copy:?}");
    assert!(copy.join("CFG.txt").exists());
    let attached = platform::git::command(&copy).args(["symbolic-ref", "-q", "HEAD"]).output().expect("git");
    assert!(!attached.status.success(), "the copy is on no branch");
    let system = request.session.append_system.clone().unwrap_or_default();
    assert!(system.contains("Read only"), "{system}");
    let branches = platform::git::command(second).args(["branch", "--list"]).output().expect("git");
    assert_eq!(String::from_utf8_lossy(&branches.stdout).lines().count(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_items_in_different_directories_work_apart() {
    let mut world = World::new();
    world.init_git();
    world.workflow("plain", r#"import { workflow } from "sdk";
export default workflow({ name: "plain", async run(ctx) { const ran = await ctx.exec("ls"); return { ls: ran.stdout }; } });"#);
    let other = world.dir.path().join("other");
    let file = other.join(brand::DIR).join("workflows/plain.ts");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    std::fs::write(&file, r#"import { workflow } from "sdk";
export default workflow({ name: "plain", async run(ctx) { const ran = await ctx.exec("ls"); return { ls: ran.stdout, other: true }; } });"#)
        .expect("write");
    git_repo(&other, "OTHER.txt");
    let kernel = world.start().await;
    let here = kernel.create_item("plain", json!({ "title": "a" })).expect("item");
    let app =
        json!({ "id": "o", "name": "Outra", "repositories": [{ "id": "o", "directory": other.to_string_lossy() }] });
    let there = kernel.create_item("plain", json!({ "title": "b", "application": app })).expect("item");
    let here = output(&ended(&kernel, &here).await);
    let there = output(&ended(&kernel, &there).await);
    assert!(here["ls"].as_str().is_some_and(|text| text.contains("README")) && here["other"].is_null());
    assert!(there["ls"].as_str().is_some_and(|text| text.contains("OTHER.txt")) && there["other"] == true);
}

const ENVIRONMENT: &str = r#"
import { workflow } from "sdk";

export default workflow({
  name: "ambiente",
  async run(ctx) {
    await ctx.agent("worker", "faça");
    const ran = await ctx.exec("printf %s \"$APP_URL\"", { repository: "web" });
    return { url: ran.stdout };
  },
});
"#;

#[tokio::test(flavor = "multi_thread")]
async fn the_application_s_variables_reach_its_commands_and_agents_without_entering_the_item() {
    let mut world = World::new();
    world.init_git();
    world.workflow("ambiente", ENVIRONMENT);
    let repository = store::Repository {
        id: "web".into(),
        directory: world.repo().to_string_lossy().into_owned(),
        e2e: vec!["npx cypress run".into()],
        e2e_directory: "e2e".into(),
        ..store::Repository::default()
    };
    let application = store::Application {
        id: "app".into(),
        name: "Loja".into(),
        repositories: vec![repository],
        env: store::Environment::from([("APP_URL".to_owned(), "http://loja.local".to_owned())]),
        ..store::Application::default()
    };
    let snapshot = crate::provenance::snapshot(&world.store(), &application);
    world.store().save_application(&application).expect("app");
    let kernel = world.start().await;

    let id = kernel.create_item("ambiente", json!({ "title": "x", "application": snapshot })).expect("item");
    let item = ended(&kernel, &id).await;
    assert_eq!(output(&item)["url"], "http://loja.local");
    assert!(item.input["application"].get("env").is_none());
    let request = &world.engine.requests()[0];
    assert!(request.host.environment.contains(&("APP_URL".to_owned(), "http://loja.local".to_owned())));
    let system = request.session.append_system.clone().unwrap_or_default();
    assert!(system.contains("End-to-end suite in `e2e`: `npx cypress run`"), "{system}");
}
