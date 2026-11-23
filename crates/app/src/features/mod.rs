mod code_map;
pub mod context;
pub(crate) mod memory;
mod output_savings;
mod sessions;
mod shell;
pub(crate) mod trace;
mod workspace;

use crate::api::{FeatureError, FeatureName, FeatureReply, FeatureRequest};
use domain::{PromptRequest, RunId};

use crate::features::context::{Context, Finish};

type Forget = fn(u64);
type Prepare = fn(&Context<'_>, RunId, PromptRequest) -> (PromptRequest, Option<Finish>);

const ALL: [(FeatureName, Forget); 7] = [
    (FeatureName::CodeMap, code_map::forget),
    (FeatureName::Memory, memory::forget),
    (FeatureName::OutputSavings, output_savings::forget),
    (FeatureName::Sessions, sessions::forget),
    (FeatureName::Shell, shell::forget),
    (FeatureName::Trace, trace::forget),
    (FeatureName::Workspace, workspace::forget),
];

const PREPARING: [(FeatureName, Prepare); 2] =
    [(FeatureName::CodeMap, code_map::run::prepare), (FeatureName::Memory, memory::run::prepare)];

pub fn handle(context: &Context<'_>, request: FeatureRequest) -> Result<FeatureReply, FeatureError> {
    let feature = request.feature();
    if !feature.on() {
        return Err(FeatureError::FeatureOff { feature });
    }
    answer(context, request).map_err(|message| FeatureError::Failed { feature, message })
}

fn answer(context: &Context<'_>, request: FeatureRequest) -> Result<FeatureReply, String> {
    match request {
        FeatureRequest::CodeMap(request) => code_map::handle(context, request).map(FeatureReply::CodeMap),
        FeatureRequest::Memory(request) => memory::handle(context, request).map(FeatureReply::Memory),
        FeatureRequest::OutputSavings => output_savings::handle(context).map(FeatureReply::OutputSavings),
        FeatureRequest::Sessions(request) => sessions::handle(context, request).map(FeatureReply::Sessions),
        FeatureRequest::Shell(request) => shell::handle(context, request).map(FeatureReply::Shell),
        FeatureRequest::Trace(request) => trace::handle(context, request).map(FeatureReply::Trace),
        FeatureRequest::Workspace(request) => workspace::handle(context, request).map(FeatureReply::Workspace),
    }
}

pub fn forget(client: u64) {
    for (_, forget) in ALL {
        forget(client);
    }
}

pub fn prepare(context: &Context<'_>, run: RunId, request: PromptRequest) -> (PromptRequest, Vec<Finish>) {
    let mut request = request;
    let mut finishes = Vec::new();
    for (feature, prepare) in PREPARING {
        if feature.on() {
            let (prepared, finish) = prepare(context, run, request);
            request = prepared;
            finishes.extend(finish);
        }
    }
    (request, finishes)
}

#[cfg(test)]
mod requests_tests;
#[cfg(test)]
mod switch_tests;

#[cfg(test)]
mod tests {
    use super::{ALL, PREPARING};

    #[test]
    fn the_registered_features_are_these_in_this_order() {
        let features: Vec<crate::api::FeatureName> = ALL.iter().map(|(feature, _)| *feature).collect();
        assert_eq!(features, crate::api::FeatureName::ALL, "one entry per feature the protocol names");
        let names: Vec<&str> = features.iter().map(|feature| feature.name()).collect();
        assert_eq!(
            names,
            [
                "code_map",
                "memory",
                "output_savings",
                "sessions",
                "shell",
                "trace",
                "workspace",
            ]
        );
    }

    #[test]
    fn these_features_prepare_a_run_in_this_order() {
        let names: Vec<&str> = PREPARING.iter().map(|(feature, _)| feature.name()).collect();
        assert_eq!(names, ["code_map", "memory"]);
    }
}
