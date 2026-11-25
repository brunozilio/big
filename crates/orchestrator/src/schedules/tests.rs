use std::time::Duration;

use jiff::Zoned;
use jiff::civil::date;
use serde_json::json;
use store::{Application, TrackerChoice};

use super::{Cron, latest};
use crate::tests::World;

const HOURS: Duration = Duration::from_secs(6 * 3600);

fn at(hour: i8, minute: i8) -> Zoned {
    date(2026, 11, 24).at(hour, minute, 17, 0).in_tz("America/Sao_Paulo").expect("zoned")
}

fn application(id: &str, security: bool) -> Application {
    Application {
        id: id.into(),
        name: format!("App {id}"),
        security: security.then(|| TrackerChoice { connection: "c1".into(), project: format!("guid-{id}") }),
        ..Application::default()
    }
}

const SWEEP: &str = r#"import { workflow, on } from "sdk";
export default workflow({
  name: "sweep",
  triggers: [on("schedule", { cron: "0 5 * * *", applications: "security", zone: "America/Sao_Paulo", title: "Varredura" })],
  async run(ctx) { return await ctx.waitHuman([]); },
});"#;

#[test]
fn the_latest_slot_is_the_last_matching_minute_inside_the_window() {
    let cron = Cron::parse("0 8 * * *").expect("cron");
    let found = latest(&cron, &at(10, 5), HOURS).expect("slot");
    assert_eq!((found.hour(), found.minute(), found.second()), (8, 0, 0));
    assert!(latest(&cron, &at(7, 59), HOURS).is_none());
    assert!(latest(&cron, &at(14, 1), HOURS).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_schedule_opens_one_item_per_application_with_the_choice_and_only_once_per_slot() {
    let world = World::new();
    world.store().save_application(&application("a", true)).expect("a");
    world.store().save_application(&application("b", false)).expect("b");
    world.workflow("sweep", SWEEP);
    let kernel = world.start().await;
    assert_eq!(kernel.tick_schedules(&at(4, 30)).expect("tick"), 0);
    assert_eq!(kernel.tick_schedules(&at(5, 0)).expect("tick"), 1);
    assert_eq!(kernel.tick_schedules(&at(6, 45)).expect("tick"), 0);
    let items = kernel.items().expect("items");
    assert_eq!(items.len(), 1);
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.workflow, "sweep");
    assert_eq!(kept.input["application"]["id"], "a");
    assert_eq!(kept.input["application"]["security"], json!({ "connection": "c1", "project": "guid-a" }));
    assert_eq!(kept.input["title"], "Varredura · App a · 2026-11-24");
    assert_eq!(kept.input["schedule"]["subject"], "schedule:sweep:a:2026-11-24T05:00");
    assert_eq!(kept.input["sources"][0]["kind"], "schedule");
    let tomorrow = date(2026, 11, 25).at(5, 1, 0, 0).in_tz("America/Sao_Paulo").expect("zoned");
    assert_eq!(kernel.tick_schedules(&tomorrow).expect("tick"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_schedule_without_applications_opens_a_single_item_and_a_bad_cron_opens_none() {
    let world = World::new();
    let plain = SWEEP.replace(", applications: \"security\"", "");
    world.workflow("sweep", &plain);
    world.workflow("broken", &SWEEP.replace("sweep", "broken").replace("0 5 * * *", "0 5 * *"));
    let kernel = world.start().await;
    assert_eq!(kernel.tick_schedules(&at(5, 3)).expect("tick"), 1);
    let items = kernel.items().expect("items");
    let kept = kernel.item(&items[0].id).expect("store").expect("item");
    assert_eq!(kept.workflow, "sweep");
    assert!(kept.input["application"].is_null());
    assert_eq!(kept.input["title"], "Varredura · 2026-11-24");
}
