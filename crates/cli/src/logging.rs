use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use server::outbound::{Line, Outbound, Shape};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};

const DEFAULT_FILTER: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";
const KEPT_BYTES: u64 = 5 * 1024 * 1024;

pub struct Redacted(pub Arc<Outbound>);

impl<'a> MakeWriter<'a> for Redacted {
    type Writer = Line<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        self.0.line()
    }
}

pub fn file_in(data: &Path) -> PathBuf {
    data.join("logs").join(format!("{}.log", brand::COMMAND))
}

pub fn filter(asked: Option<&str>) -> EnvFilter {
    asked
        .filter(|text| !text.trim().is_empty())
        .and_then(|text| EnvFilter::try_new(text).ok())
        .unwrap_or_else(|| EnvFilter::new(DEFAULT_FILTER))
}

pub fn open(file: &Path) -> std::io::Result<File> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    if std::fs::metadata(file).is_ok_and(|meta| meta.len() > KEPT_BYTES) {
        std::fs::rename(file, file.with_extension("log.1"))?;
    }
    OpenOptions::new().create(true).append(true).open(file)
}

pub fn install(terminal: bool, kept: bool) -> Option<PathBuf> {
    let path = platform::local_data_dir(brand::NAME, brand::COMMAND).map(|data| file_in(&data));
    let file = path.as_deref().and_then(|path| open(path).ok());
    let saved = file.map(|file| {
        fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(Redacted(Outbound::configured(Shape::Json, Box::new(file))))
            .with_filter(filter(Some(brand::log::FILTER)))
    });
    let shown = terminal.then(|| shown(Format::chosen(brand::log::FORMAT)));
    let trail = kept.then(server::trail::layer);
    tracing_subscriber::registry().with(saved).with(shown).with(trail).try_init().ok()?;
    log_panics();
    path
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

impl Format {
    pub fn chosen(text: &str) -> Format {
        if text.trim().eq_ignore_ascii_case("json") { Format::Json } else { Format::Text }
    }
}

fn shown<S>(format: Format) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let filter = filter(Some(brand::log::TERMINAL));
    match format {
        Format::Json => fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(Redacted(Outbound::configured(Shape::Json, Box::new(std::io::stderr()))))
            .with_filter(filter)
            .boxed(),
        Format::Text => fmt::layer()
            .with_writer(Redacted(Outbound::configured(Shape::Text, Box::new(std::io::stderr()))))
            .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
            .with_timer(ChronoLocal::new(TIME_FORMAT.to_owned()))
            .with_target(false)
            .with_filter(filter)
            .boxed(),
    }
}

fn log_panics() {
    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let thread = std::thread::current();
        tracing::error!(thread = thread.name().unwrap_or("unnamed"), "{panic}");
        earlier(panic);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_sits_in_the_data_folder_under_the_command() {
        let file = file_in(Path::new("/data"));
        assert_eq!(file, Path::new("/data/logs").join(format!("{}.log", brand::COMMAND)));
    }

    #[test]
    fn warnings_are_kept_unless_another_filter_is_asked() {
        assert_eq!(filter(None).to_string(), "warn");
        assert_eq!(filter(Some("  ")).to_string(), "warn");
        assert_eq!(filter(Some("app=debug")).to_string(), "app=debug");
        assert_eq!(filter(Some("=[")).to_string(), "warn");
    }

    #[test]
    fn the_terminal_shows_text_unless_json_is_asked() {
        assert_eq!(Format::chosen("json"), Format::Json);
        assert_eq!(Format::chosen(" JSON "), Format::Json);
        assert_eq!(Format::chosen("text"), Format::Text);
        assert_eq!(Format::chosen(""), Format::Text);
    }

    #[test]
    fn a_connector_log_reaches_the_file_redacted_with_the_policy_revision() {
        let dir = tempfile::tempdir().expect("dir");
        let file = file_in(dir.path());
        let outbound = Outbound::configured(Shape::Json, Box::new(open(&file).expect("open")));
        let layer = fmt::layer().json().flatten_event(true).with_writer(Redacted(Arc::clone(&outbound)));
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "connector", "{}", "Bearer abc");
        });
        let written = std::fs::read_to_string(&file).expect("log");
        let line: serde_json::Value = serde_json::from_str(written.trim_end()).expect("one json line");
        assert_eq!(line["message"], "Bearer [redacted]", "{written}");
        assert_eq!(line["policy_revision"].as_str(), Some(outbound.revision().as_str()));
    }

    #[test]
    fn a_large_log_is_moved_aside_before_writing() {
        let dir = tempfile::tempdir().expect("dir");
        let file = file_in(dir.path());
        open(&file).expect("first");
        std::fs::write(&file, vec![b'x'; (KEPT_BYTES + 1) as usize]).expect("fill");
        open(&file).expect("again");
        assert_eq!(std::fs::metadata(&file).expect("new").len(), 0);
        assert!(file.with_extension("log.1").exists());
    }
}
