mod browser;
pub(crate) mod code_map;
pub mod context;
pub(crate) mod history;
pub(crate) mod memory;
pub(crate) mod trace;

use domain::{PromptRequest, RunId};

use crate::features::context::{Context, Finish};

type Prepare = fn(&Context<'_>, RunId, PromptRequest) -> (PromptRequest, Option<Finish>);

const PREPARING: [Prepare; 3] = [code_map::run::prepare, memory::run::prepare, browser::prepare];

pub fn prepare(context: &Context<'_>, run: RunId, request: PromptRequest) -> (PromptRequest, Vec<Finish>) {
    let mut request = request;
    let mut finishes = Vec::new();
    for prepare in PREPARING {
        let (prepared, finish) = prepare(context, run, request);
        request = prepared;
        finishes.extend(finish);
    }
    (request, finishes)
}
