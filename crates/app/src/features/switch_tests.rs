use std::path::Path;
use std::sync::Arc;

use crate::api::features::channels::ChannelsRequest;
use crate::api::features::code_map::CodeMapRequest;
use crate::api::features::memory::{MemoryOp, MemoryRequest};
use crate::api::features::plugins::PluginsRequest;
use crate::api::features::schedules::SchedulesRequest;
use crate::api::features::sessions::SessionsRequest;
use crate::api::features::shell::ShellRequest;
use crate::api::features::trace::TraceRequest;
use crate::api::features::workspace::{WorkspaceOp, WorkspaceRequest};
use crate::api::{FeatureError, FeatureName, FeatureRequest};
use crate::features::context::testing;
use crate::services::LocalServices;

const CLIENT: u64 = 96_301;

fn a_request_to(feature: FeatureName, cwd: &Path) -> FeatureRequest {
    let path = cwd.to_path_buf();
    match feature {
        FeatureName::Channels => FeatureRequest::Channels(ChannelsRequest { path, names: Vec::new() }),
        FeatureName::CodeMap => FeatureRequest::CodeMap(CodeMapRequest { path, prompt: "retry".into() }),
        FeatureName::Memory => FeatureRequest::Memory(MemoryRequest { path, op: MemoryOp::List }),
        FeatureName::OutputSavings => FeatureRequest::OutputSavings,
        FeatureName::Plugins => FeatureRequest::Plugins(PluginsRequest::List),
        FeatureName::Schedules => FeatureRequest::Schedules(SchedulesRequest::List),
        FeatureName::Sessions => FeatureRequest::Sessions(SessionsRequest::List),
        FeatureName::Shell => FeatureRequest::Shell(ShellRequest::List),
        FeatureName::Trace => FeatureRequest::Trace(TraceRequest::EverySession),
        FeatureName::Workspace => FeatureRequest::Workspace(WorkspaceRequest { path, op: WorkspaceOp::GitOffer }),
    }
}

fn is_off(services: &LocalServices, feature: FeatureName, cwd: &Path) -> bool {
    let asked = testing::with(CLIENT, services, |context| super::handle(context, a_request_to(feature, cwd)));
    matches!(asked, Err(FeatureError::FeatureOff { .. }))
}

#[test]
fn each_feature_is_off_only_when_the_config_turns_it_off() {
    let repo = tempfile::tempdir().expect("repo");
    let services = testing::services(None);
    for feature in FeatureName::ALL {
        assert_eq!(is_off(&services, feature, repo.path()), !feature.on(), "{feature}");
    }
    super::forget(CLIENT);
}

#[test]
fn the_settings_turn_no_feature_off() {
    let (home, repo) = (tempfile::tempdir().expect("home"), tempfile::tempdir().expect("repo"));
    std::fs::create_dir_all(home.path().join(brand::DIR)).expect("dir");
    let every_folder: serde_json::Map<String, serde_json::Value> =
        FeatureName::ALL.iter().map(|feature| (feature.name().to_owned(), false.into())).collect();
    let settings = serde_json::json!({ "features": every_folder });
    std::fs::write(home.path().join(brand::DIR).join("settings.json"), settings.to_string()).expect("write");
    let mut services = testing::services(None);
    services.homes = Arc::new(crate::OwnerHome::at(home.path()));

    for feature in FeatureName::ALL {
        assert_eq!(is_off(&services, feature, repo.path()), !feature.on(), "{feature}");
    }
    super::forget(CLIENT);
}
