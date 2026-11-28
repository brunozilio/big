use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use database::{Database, Lease, Target};
use store::Store;

const LEADER: &str = "leader";
const LEASE_SECONDS: i64 = 15;
const RENEW_EVERY: Duration = Duration::from_secs(3);
const ASK_EVERY: Duration = Duration::from_secs(1);

static DRAINING: AtomicBool = AtomicBool::new(false);

fn configured(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map(|text| text.trim().to_owned()).map_err(|error| format!("{path}: {error}"))
}

pub fn cloud() -> bool {
    configured(brand::database::URL_FILE).is_some()
}

pub fn draining() -> bool {
    DRAINING.load(Ordering::SeqCst)
}

fn region_of(arn: &str) -> Option<String> {
    arn.split(':').nth(3).filter(|region| !region.is_empty()).map(str::to_owned)
}

pub fn password_in(secret: &str) -> String {
    serde_json::from_str::<serde_json::Value>(secret)
        .ok()
        .and_then(|value| value["password"].as_str().map(str::to_owned))
        .unwrap_or_else(|| secret.to_owned())
}

fn password() -> Result<Option<database::Password>, String> {
    let Some(file) = configured(brand::database::PASSWORD_SECRET_FILE) else { return Ok(None) };
    let arn = read(&file)?;
    let region = region_of(&arn).ok_or_else(|| format!("{file}: not a secret ARN"))?;
    Ok(Some(Arc::new(move || platform::aws::secret_string(&region, &arn).map(|secret| password_in(&secret)))))
}

pub fn connect() -> Result<Database, String> {
    let file = configured(brand::database::URL_FILE).ok_or("no database configured")?;
    let target = Target { url: read(&file)?, ca_file: configured(brand::database::CA_FILE).unwrap_or_default() };
    let opened = match password()? {
        Some(password) => Database::postgres_with_password(&target, password),
        None => Database::postgres(&target),
    };
    opened.map_err(|error| format!("the database could not be opened: {error}"))
}

pub fn copy_local(data: &Path, store: &Store) {
    let file = data.join("app.db");
    if !file.is_file() {
        return;
    }
    let source = match Store::open(&file, 0) {
        Ok(source) => source,
        Err(error) => return tracing::warn!(%error, "the local database could not be read to copy it"),
    };
    match database::copy_into(source.database(), store.database(), &[]) {
        Ok(Some(copied)) => tracing::info!(tables = copied.tables, rows = copied.rows, "the local database was copied"),
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "the local database could not be copied"),
    }
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

pub fn lease() -> Lease {
    let holder = format!("{}-{}", std::process::id(), jiff::Timestamp::now().as_nanosecond());
    Lease::new(LEADER, holder, LEASE_SECONDS)
}

pub async fn take_turn(store: &Arc<Store>, lease: &Lease) {
    let mut told = false;
    loop {
        let (store, mine) = (Arc::clone(store), lease.clone());
        let turn = tokio::task::spawn_blocking(move || mine.take(store.database(), now())).await;
        match turn {
            Ok(Ok(turn)) if turn.held => return,
            Ok(Err(error)) => tracing::warn!(%error, "the turn could not be asked for"),
            _ => {}
        }
        if !told {
            tracing::info!("waiting for the running server to hand over its turn");
            told = true;
        }
        tokio::time::sleep(ASK_EVERY).await;
    }
}

pub fn keep_turn(store: Arc<Store>, lease: Lease, step_down: impl FnOnce() + Send + 'static) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RENEW_EVERY).await;
            let (held, mine) = (Arc::clone(&store), lease.clone());
            let turn = tokio::task::spawn_blocking(move || mine.take(held.database(), now())).await;
            let leave = match turn {
                Ok(Ok(turn)) => !turn.held || turn.wanted,
                _ => false,
            };
            if leave {
                tracing::info!("another server asked for the turn; this one stops its work and drains");
                DRAINING.store(true, Ordering::SeqCst);
                step_down();
                let mine = lease.clone();
                let _ = tokio::task::spawn_blocking(move || mine.release(store.database())).await;
                return;
            }
        }
    });
}

pub fn release(store: &Store, lease: &Lease) {
    if let Err(error) = lease.release(store.database()) {
        tracing::warn!(%error, "the turn could not be released");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_postgres(url: &str) -> Database {
        let schema = format!("t_{}_{}", std::process::id(), jiff::Timestamp::now().as_nanosecond());
        let admin = Database::postgres(&Target { url: url.to_owned(), ca_file: String::new() })
            .unwrap_or_else(|error| panic!("{error}"));
        admin.write(|conn| conn.batch(&format!("CREATE SCHEMA {schema}"))).unwrap_or_else(|error| panic!("{error}"));
        let joined = if url.contains('?') { '&' } else { '?' };
        let target = Target { url: format!("{url}{joined}options=-csearch_path%3D{schema}"), ca_file: String::new() };
        Database::postgres(&target).unwrap_or_else(|error| panic!("{error}"))
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs a PostgreSQL in DATABASE_TEST_URL"]
    async fn the_local_file_is_copied_and_the_turn_moves_to_the_new_server() {
        let Some(url) = platform::environment::var("DATABASE_TEST_URL") else { return };
        let data = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let local = Store::open(&data.path().join("app.db"), 0).unwrap_or_else(|error| panic!("{error}"));
        local.set_preference("kept", &serde_json::json!("from the file")).unwrap_or_else(|error| panic!("{error}"));
        drop(local);
        let store = Arc::new(Store::with_database(fresh_postgres(&url), 0).unwrap_or_else(|error| panic!("{error}")));
        copy_local(data.path(), &store);
        assert_eq!(store.preference("kept").ok().flatten(), Some(serde_json::json!("from the file")));
        let old = lease();
        take_turn(&store, &old).await;
        let stepped = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&stepped);
        keep_turn(Arc::clone(&store), old, move || seen.store(true, Ordering::SeqCst));
        let new = Lease::new(LEADER, "new", LEASE_SECONDS);
        let waited = tokio::time::timeout(Duration::from_secs(20), take_turn(&store, &new)).await;
        assert!(waited.is_ok(), "the new server never got the turn");
        assert!(stepped.load(Ordering::SeqCst));
        assert!(draining());
    }

    #[test]
    fn the_password_comes_from_the_managed_secret_or_is_the_secret_itself() {
        assert_eq!(password_in(r#"{"username":"app","password":"s3cr3t"}"#), "s3cr3t");
        assert_eq!(password_in("plain"), "plain");
        assert_eq!(
            region_of("arn:aws:secretsmanager:us-east-1:123456789012:secret:rds!db-1"),
            Some("us-east-1".to_owned())
        );
        assert_eq!(region_of("nope"), None);
    }
}
