use domain::permission::{Allowed, Bounds, Call, Effect, Grant, PolicyContext, PolicyEngine, Remembered, Verdict};
use serde_json::json;

fn verdicts(effects: &[Effect]) -> Vec<Verdict> {
    let bounds = Bounds::of(Vec::new(), Vec::new());
    let remembered = Remembered::default();
    let listed = Allowed::of([Grant::Tool("t".into())]);
    let mut found = Vec::new();
    for mode in domain::PermissionMode::ALL {
        for allowed in [None, Some(&listed)] {
            for no_prompts in [false, true] {
                let call = Call { tool: "t", subject: "", effects };
                let cx = PolicyContext {
                    mode,
                    ceiling: None,
                    remembered: &remembered,
                    bounds: &bounds,
                    no_prompts,
                    allowed,
                };
                found.push(cx.settled(PolicyEngine::decide(&call, &cx), "t"));
            }
        }
    }
    found
}

#[test]
fn shipped_actions_are_judged_as_before_except_the_ones_that_declare_a_strict_effect() {
    let server = domain::mcp::connectors_server();
    let mut changed = Vec::new();
    for provider in connectors::scripted::discover() {
        for action in provider.descriptor().actions.iter().filter(|action| !action.hidden()) {
            let now: Vec<Effect> =
                action.declaration().operation(&server, &json!({})).map(Effect::Operation).into_iter().collect();
            let before: Vec<Effect> =
                if action.writes { vec![Effect::Remote { server: server.clone() }] } else { Vec::new() };
            if verdicts(&now) != verdicts(&before) {
                changed.push(action.name.clone());
            }
        }
    }
    changed.sort();
    changed.dedup();
    assert_eq!(changed, ["email.create_event", "email.reply", "email.send"]);
}
