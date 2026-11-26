use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::LocalError;
use crate::store::{PART, folder, relative};

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Request {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Transfer {
    pub id: String,
    pub received: u64,
    pub total: Option<u64>,
    pub running: bool,
    pub error: Option<String>,
}

fn table() -> &'static Mutex<HashMap<String, Transfer>> {
    static TABLE: std::sync::OnceLock<Mutex<HashMap<String, Transfer>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn update(id: &str, change: impl FnOnce(&mut Transfer)) {
    let mut table = table().lock().unwrap_or_else(PoisonError::into_inner);
    let entry = table.entry(id.to_owned()).or_insert_with(|| Transfer { id: id.to_owned(), ..Transfer::default() });
    change(entry);
}

pub fn transfers() -> Vec<Transfer> {
    let table = table().lock().unwrap_or_else(PoisonError::into_inner);
    let mut listed: Vec<Transfer> = table.values().cloned().collect();
    listed.sort_by(|a, b| a.id.cmp(&b.id));
    listed
}

pub fn start(request: Request) -> Result<Transfer, LocalError> {
    let path = folder()?.join(relative(&request.name)?);
    if !request.url.starts_with("https://") && !request.url.starts_with("http://127.0.0.1") {
        return Err(LocalError::Bad(format!("{} is not an https address", request.url)));
    }
    if path.is_file() {
        let bytes = path.metadata().map(|meta| meta.len()).unwrap_or_default();
        return Ok(Transfer { id: request.name, received: bytes, total: Some(bytes), running: false, error: None });
    }
    let running = transfers().into_iter().find(|transfer| transfer.id == request.name && transfer.running);
    if let Some(running) = running {
        return Ok(running);
    }
    let id = request.name.clone();
    update(&id, |transfer| {
        transfer.running = true;
        transfer.error = None;
        transfer.total = request.bytes;
    });
    crate::runtime()?.spawn(async move {
        let id = request.name.clone();
        if let Err(error) = fetch(&request, &path).await {
            tracing::warn!(model = %id, %error, "a model download stopped");
            update(&id, |transfer| transfer.error = Some(error.to_string()));
        }
        update(&id, |transfer| transfer.running = false);
    });
    Ok(transfers().into_iter().find(|transfer| transfer.id == id).unwrap_or_default())
}

fn part_of(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}{PART}", path.display()))
}

async fn fetch(request: &Request, path: &Path) -> Result<(), LocalError> {
    let disk = |error: std::io::Error| LocalError::Disk(error.to_string());
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(disk)?;
    }
    let part = part_of(path);
    let mut offset = tokio::fs::metadata(&part).await.map(|meta| meta.len()).unwrap_or_default();
    let client = reqwest::Client::builder().build().map_err(|error| LocalError::Network(error.to_string()))?;
    let mut call = client.get(&request.url);
    for (name, value) in &request.headers {
        call = call.header(name, value);
    }
    if offset > 0 {
        call = call.header("range", format!("bytes={offset}-"));
    }
    let response = call.send().await.map_err(|error| LocalError::Network(error.to_string()))?;
    let status = response.status().as_u16();
    if status == 416 && request.bytes.is_some_and(|bytes| bytes == offset) {
        return tokio::fs::rename(&part, path).await.map_err(disk);
    }
    if !(200..300).contains(&status) {
        return Err(LocalError::Network(format!("{} answered HTTP {status}", request.url)));
    }
    if status != 206 {
        offset = 0;
    }
    let total = response.content_length().map(|length| length + offset).or(request.bytes);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(&part)
        .await
        .map_err(disk)?;
    let id = request.name.clone();
    update(&id, |transfer| {
        transfer.received = offset;
        transfer.total = total;
    });
    let mut received = offset;
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|error| LocalError::Network(error.to_string()))?;
        file.write_all(&chunk).await.map_err(disk)?;
        received += chunk.len() as u64;
        update(&id, |transfer| transfer.received = received);
    }
    file.flush().await.map_err(disk)?;
    if total.is_some_and(|total| total != received) {
        return Err(LocalError::Network(format!("the download ended at {received} of {total:?} bytes")));
    }
    tokio::fs::rename(&part, path).await.map_err(disk)
}
