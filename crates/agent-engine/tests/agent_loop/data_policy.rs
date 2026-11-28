use agent_engine::agent::{Agent, Ending};
use agent_engine::model::scripted::ScriptedModel;
use agent_engine::model::{ModelError, TurnRequest};
use agent_engine::tools;
use agent_engine::transcript::{Block, Message};
use domain::data_policy::{Requires, Rerouted, Skipped};
use domain::{EngineEvent, ModelProvider};

use crate::{Rig, request};

const PRIMARY: &str = "keyed-too:primary";
const US: &str = "keyed:us-model";
const EU: &str = "keyed-too:eu-model";

fn rig(fallbacks: &[&str], requires: Requires) -> Rig {
    agent_engine::providers::install_testing();
    let mut rig = Rig::new(domain::Reviewer::Human);
    rig.workspace.models.model.provider = ModelProvider::new("keyed-too");
    rig.workspace.models.fallbacks = fallbacks.iter().map(|model| (*model).to_owned()).collect();
    rig.workspace.models.requires = requires;
    rig
}

fn eu() -> Requires {
    Requires { regions: Some(vec!["eu".into()]), ..Requires::default() }
}

fn busy_then_ok() -> ScriptedModel {
    ScriptedModel::new([
        Err(ModelError::Overloaded("busy".into())),
        ScriptedModel::reply(vec![Block::Text("ok".into())]),
    ])
}

fn run(rig: &Rig, model: &ScriptedModel) -> Result<(), Ending> {
    run_on(rig, model, PRIMARY)
}

fn run_on(rig: &Rig, model: &ScriptedModel, first: &str) -> Result<(), Ending> {
    let tools = tools::standard();
    let events = std::sync::Arc::clone(&rig.events);
    let emit = move |event: EngineEvent| events.lock().expect("events").push(event);
    let agent = Agent {
        api: model,
        tools: &tools,
        workspace: &rig.workspace,
        control: &rig.control,
        emit: &emit,
        request: TurnRequest { model: first.into(), tools: Agent::specs(&tools), ..request() },
        agents: &[],
        depth: 0,
        turn_limit: agent_engine::agent::MAX_TURNS,
    };
    agent.run(&mut vec![Message::user("go")]).map(|_| ())
}

fn asked(model: &ScriptedModel) -> Vec<String> {
    model.asked().into_iter().map(|request| request.model).collect()
}

fn decisions(rig: &Rig) -> Vec<Rerouted> {
    let found = rig.events().into_iter().filter_map(|event| match event {
        EngineEvent::Rerouted(rerouted) => Some(rerouted),
        _ => None,
    });
    found.collect()
}

#[test]
fn the_fallback_skips_a_model_outside_the_required_region() {
    let rig = rig(&[US, EU], eu());
    let model = busy_then_ok();
    run(&rig, &model).expect("answered");
    assert_eq!(asked(&model), [PRIMARY, EU]);
    let skipped = vec![Skipped { model: US.into(), why: "region us-east-1 is not eu".into() }];
    let decision = Rerouted { from: PRIMARY.into(), to: Some(EU.into()), failure: "overloaded".into(), skipped };
    assert_eq!(decisions(&rig), [decision]);
    assert!(rig.events().contains(&EngineEvent::FellBack(EU.into())));
}

#[test]
fn without_a_model_that_meets_the_policy_the_run_fails_with_data_policy_unmet() {
    let rig = rig(&[US], eu());
    let model = busy_then_ok();
    let ended = run(&rig, &model);
    let Err(Ending::Failed(ModelError::DataPolicyUnmet(why))) = ended else { panic!("{ended:?}") };
    assert_eq!(why, "keyed:us-model: region us-east-1 is not eu");
    assert_eq!(asked(&model), [PRIMARY]);
    assert_eq!(decisions(&rig)[0].to, None);
}

#[test]
fn a_session_model_outside_the_policy_is_never_asked() {
    let mut rig = rig(&[EU], eu());
    rig.workspace.models.model.provider = ModelProvider::new("keyed");
    let model = ScriptedModel::new([ScriptedModel::reply(vec![Block::Text("ok".into())])]);
    run_on(&rig, &model, "m").expect("answered");
    assert_eq!(decisions(&rig)[0].failure, "data_policy_unmet");
    assert_eq!(asked(&model), [EU]);
}

#[test]
fn nothing_required_keeps_the_chain_as_it_was() {
    let rig = rig(&[US, EU], Requires::default());
    let model = busy_then_ok();
    run(&rig, &model).expect("answered");
    assert_eq!(asked(&model), [PRIMARY, US]);
    let decision =
        Rerouted { from: PRIMARY.into(), to: Some(US.into()), failure: "overloaded".into(), skipped: vec![] };
    assert_eq!(decisions(&rig), [decision]);
}
