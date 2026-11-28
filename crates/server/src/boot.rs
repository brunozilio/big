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
    let every = brand::store::SNAPSHOT_EVERY.trim().parse().unwrap_or(0);
    let cloud = crate::cloud_db::cloud();
    let (store, services) = if cloud {
        let store = Store::with_database(crate::cloud_db::connect()?, every).map_err(|error| error.to_string())?;
        let services = app::LocalServices::detect_on(crate::cloud_db::connect()?);
        crate::cloud_db::copy_local(&launch.data, &store);
        (Arc::new(store), services)
    } else {
        let store = Store::open(&launch.data.join("app.db"), every).map_err(|error| error.to_string())?;
        (Arc::new(store), app::LocalServices::detect())
    };
    let lease = crate::cloud_db::lease();
    if cloud {
        crate::cloud_db::take_turn(&store, &lease).await;
    }
    crate::permissions::install(Arc::clone(&store));
    crate::scope::load_claimed(&store);
    let http = Arc::new(ReqwestHttp::new().map_err(|error| error.message)?);
    let tokens = Arc::new(VaultTokens::new(Arc::clone(&services.secrets)));
    let keeper = TokenKeeper::new(tokens, http.clone(), Arc::new(SystemClock));
    let sink = Arc::new(StoredSettings(Arc::clone(&store)));
    let calls = Arc::new(crate::usage_calls::StoredCalls(Arc::clone(&store)));
    let connectors = Arc::new(Connectors::new(http, keeper).with_sink(sink).with_calls(calls));
    app::install_spend(Arc::new(crate::usage_calls::StoredSpend(Arc::clone(&store))));
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
    if cloud {
        let stopping = (*handle).clone();
        crate::cloud_db::keep_turn(Arc::clone(&store), lease.clone(), move || stopping.shutdown());
    }
    let remembered = (client.clone(), Arc::clone(&engine));
    std::thread::spawn(move || {
        if let Ok(models) = remembered.0.models() {
            remembered.1.remember(models);
        }
    });
    let kept = Arc::clone(&store);
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
    if cloud {
        crate::cloud_db::release(&kept, &lease);
    }
    served.map_err(|error| error.to_string())
}
