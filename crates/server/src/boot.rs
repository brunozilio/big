use std::future::Future;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use connectors::{Connectors, ReqwestHttp, SystemClock, TokenKeeper};
use orchestrator::{Config, Orchestrator};
use store::Store;

use crate::backend::Backend;
use crate::connections::StoredSettings;
use crate::vault::VaultTokens;

#[derive(Debug)]
pub struct Launch {
    pub repo: PathBuf,
    pub data: PathBuf,
    pub listener: TcpListener,
}

pub async fn run(launch: Launch, stop: impl Future<Output = ()> + Send + 'static) -> Result<(), String> {
    let services = app::LocalServices::detect();
    let every = brand::store::SNAPSHOT_EVERY.trim().parse().unwrap_or(0);
    let store = Arc::new(Store::open(&launch.data.join("app.db"), every).map_err(|error| error.to_string())?);
    crate::permissions::install(Arc::clone(&store));
    crate::scope::load_claimed(&store);
    let http = Arc::new(ReqwestHttp::new().map_err(|error| error.message)?);
    let tokens = Arc::new(VaultTokens::new(Arc::clone(&services.secrets)));
    let keeper = TokenKeeper::new(tokens, http.clone(), Arc::new(SystemClock));
    let sink = Arc::new(StoredSettings(Arc::clone(&store)));
    let connectors = Arc::new(Connectors::new(http, keeper).with_sink(sink));
    connectors::memory::install(Arc::new(crate::connector_memory::StoredMemory(Arc::clone(&store))));
    let runtime = tokio::runtime::Handle::current();
    let connected = crate::connected_mcp::for_runs(Arc::clone(&store), Arc::clone(&connectors), runtime);
    let address = launch.listener.local_addr().map_err(|error| error.to_string())?;
    let hub = browser::Hub::new();
    connectors::browser::install(Arc::new(crate::connector_browser::Browsers::new(
        hub.clone(),
        Arc::clone(&store),
        browser::Settings::configured(&launch.data),
    )));
    let services = services
        .with_applications(crate::memories::applications(Arc::clone(&store)))
        .with_connected_mcp(connected)
        .with_browser_mcp(crate::browsers::for_runs(&hub, &launch.data, Arc::clone(&store)));
    crate::browsers::install(&hub, &launch.data, address.port());
    if let Err(error) = crate::mcp::token(&launch.data) {
        tracing::warn!(%error, "the token other agents use for the MCP resources could not be written");
    }
    let client = app::Client::builder().services(services.clone()).build().map_err(|error| error.to_string())?;
    let engine = Arc::new(app::KeyedEngine::new(services.clone()));
    let config = Config::new(Arc::clone(&store), Arc::clone(&connectors), engine.clone(), launch.data.clone())
        .in_repo(launch.repo.clone())
        .with_server(format!("http://127.0.0.1:{}", address.port()));
    let handle = Orchestrator::start(config).map_err(|error| error.to_string())?;
    let remembered = (client.clone(), Arc::clone(&engine));
    std::thread::spawn(move || {
        if let Ok(models) = remembered.0.models() {
            remembered.1.remember(models);
        }
    });
    let backend = Backend {
        orchestrator: (*handle).clone(),
        store,
        connectors,
        client,
        engine: Some(engine),
        data: launch.data,
        browser: hub,
    };
    let served = crate::server::serve(launch.listener, backend, page::embedded(), stop).await;
    drop(handle);
    served.map_err(|error| error.to_string())
}
