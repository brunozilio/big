use domain::origin::{Attribution, Origin};
use domain::workflow::ItemId;

use crate::protocol::Command;

pub fn attribution(item: &ItemId, step: &str, command: &Command) -> Attribution {
    let origin = command.args["origin"].as_str().and_then(Origin::named).unwrap_or_default();
    Attribution::of_step(item.to_string(), step, origin)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn command(args: serde_json::Value) -> Command {
        serde_json::from_value(json!({ "seq": 1, "kind": "agent", "branch": "", "args": args })).expect("command")
    }

    #[test]
    fn a_run_is_charged_to_its_item_and_step_and_to_the_origin_the_workflow_names() {
        let item = ItemId::from("i1");
        let plain = attribution(&item, "develop", &command(json!({ "name": "dev" })));
        assert_eq!(plain, Attribution::of_step("i1", "develop", Origin::Step));
        let eval = attribution(&item, "evals", &command(json!({ "name": "judge", "origin": "eval" })));
        assert_eq!(eval.origin, Origin::Eval);
        let unknown = attribution(&item, "x", &command(json!({ "origin": "whatever" })));
        assert_eq!(unknown.origin, Origin::Step);
    }
}
