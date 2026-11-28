use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bytes::BytesMut;
use postgres::types::{IsNull, ToSql, Type, to_sql_checked};
use postgres::{Client, GenericClient, NoTls};
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

use crate::sql::for_postgres;
use crate::{Conn, Dialect, Error, Result, Row, Target, Value};

type Boxed = Box<dyn std::error::Error + Sync + Send>;

pub type Password = Arc<dyn Fn() -> std::result::Result<String, String> + Send + Sync>;

pub struct Postgres {
    target: Target,
    password: Option<Password>,
    client: Mutex<Option<Client>>,
}

impl Drop for Postgres {
    fn drop(&mut self) {
        let client = self.client.get_mut().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(client) = client {
            std::thread::scope(|scope| {
                let _ = scope.spawn(move || drop(client)).join();
            });
        }
    }
}

impl std::fmt::Debug for Postgres {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Postgres").finish_non_exhaustive()
    }
}

fn opened(target: &Target, password: Option<&Password>) -> Result<Client> {
    let mut config: postgres::Config = target.url.parse()?;
    if let Some(password) = password {
        config.password(password().map_err(Error::Setup)?);
    }
    if target.ca_file.trim().is_empty() {
        return Ok(config.connect(NoTls)?);
    }
    let mut roots = rustls::RootCertStore::empty();
    let certificates = CertificateDer::pem_file_iter(target.ca_file.trim())
        .map_err(|error| Error::Setup(format!("{}: {error}", target.ca_file)))?;
    for certificate in certificates {
        let certificate = certificate.map_err(|error| Error::Setup(format!("{}: {error}", target.ca_file)))?;
        roots.add(certificate).map_err(|error| Error::Setup(error.to_string()))?;
    }
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|error| Error::Setup(error.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config.connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls))?)
}

impl Postgres {
    pub fn connect(target: &Target, password: Option<Password>) -> Result<Self> {
        let made = Self { target: target.clone(), password, client: Mutex::new(None) };
        let first = std::thread::scope(|scope| scope.spawn(|| opened(target, made.password.as_ref())).join());
        let client = first.map_err(|_| Error::Setup("the database connection thread stopped".to_owned()))??;
        *made.held() = Some(client);
        Ok(made)
    }

    fn held(&self) -> MutexGuard<'_, Option<Client>> {
        self.client.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn run<R: Send>(&self, transactional: bool, work: impl FnOnce(&mut dyn Conn) -> Result<R> + Send) -> Result<R> {
        let mut held = self.held();
        let target = &self.target;
        let password = self.password.as_ref();
        let slot = &mut *held;
        let answer = std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    if slot.as_ref().is_none_or(Client::is_closed) {
                        *slot = Some(opened(target, password)?);
                    }
                    let client = slot.as_mut().ok_or_else(|| Error::Setup("no database connection".to_owned()))?;
                    if !transactional {
                        return work(&mut Pg(client));
                    }
                    let mut transaction = client.transaction()?;
                    let done = work(&mut Pg(&mut transaction))?;
                    transaction.commit()?;
                    Ok(done)
                })
                .join()
        });
        answer.map_err(|_| Error::Setup("the database thread stopped".to_owned()))?
    }
}

struct Pg<'a, C: GenericClient>(&'a mut C);

struct Bound<'a>(&'a Value);

impl std::fmt::Debug for Bound<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl ToSql for Bound<'_> {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> std::result::Result<IsNull, Boxed> {
        match self.0 {
            Value::Null => Ok(IsNull::Yes),
            Value::Integer(number) => integer(*number, ty, out),
            Value::Real(number) => match *ty {
                Type::FLOAT4 => (*number as f32).to_sql(ty, out),
                _ => number.to_sql(ty, out),
            },
            Value::Text(text) => text.as_str().to_sql(ty, out),
            Value::Blob(bytes) => match *ty {
                Type::TEXT | Type::VARCHAR => std::str::from_utf8(bytes)?.to_sql(ty, out),
                _ => bytes.as_slice().to_sql(ty, out),
            },
        }
    }

    fn accepts(_: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

fn integer(number: i64, ty: &Type, out: &mut BytesMut) -> std::result::Result<IsNull, Boxed> {
    match *ty {
        Type::INT2 => i16::try_from(number)?.to_sql(ty, out),
        Type::INT4 => i32::try_from(number)?.to_sql(ty, out),
        Type::BOOL => (number != 0).to_sql(ty, out),
        Type::FLOAT8 => (number as f64).to_sql(ty, out),
        Type::FLOAT4 => (number as f32).to_sql(ty, out),
        Type::TEXT | Type::VARCHAR => number.to_string().as_str().to_sql(ty, out),
        _ => number.to_sql(ty, out),
    }
}

fn column(row: &postgres::Row, index: usize) -> Result<Value> {
    let ty = row.columns().get(index).map(|column| column.type_().clone()).unwrap_or(Type::TEXT);
    let value = match ty {
        Type::INT8 => row.try_get::<_, Option<i64>>(index)?.map(Value::Integer),
        Type::INT4 => row.try_get::<_, Option<i32>>(index)?.map(|number| Value::Integer(i64::from(number))),
        Type::INT2 => row.try_get::<_, Option<i16>>(index)?.map(|number| Value::Integer(i64::from(number))),
        Type::BOOL => row.try_get::<_, Option<bool>>(index)?.map(|flag| Value::Integer(i64::from(flag))),
        Type::FLOAT8 => row.try_get::<_, Option<f64>>(index)?.map(Value::Real),
        Type::FLOAT4 => row.try_get::<_, Option<f32>>(index)?.map(|number| Value::Real(f64::from(number))),
        Type::BYTEA => row.try_get::<_, Option<Vec<u8>>>(index)?.map(Value::Blob),
        Type::NUMERIC => return Err(Error::decode("a NUMERIC column needs a cast to BIGINT or DOUBLE PRECISION")),
        _ => row.try_get::<_, Option<String>>(index)?.map(Value::Text),
    };
    Ok(value.unwrap_or(Value::Null))
}

impl<C: GenericClient> Conn for Pg<'_, C> {
    fn dialect(&self) -> Dialect {
        Dialect::Postgres
    }

    fn execute(&mut self, sql: &str, args: &[Value]) -> Result<u64> {
        let bound: Vec<Bound<'_>> = args.iter().map(Bound).collect();
        let refs: Vec<&(dyn ToSql + Sync)> = bound.iter().map(|value| value as &(dyn ToSql + Sync)).collect();
        Ok(self.0.execute(for_postgres(sql).as_str(), &refs)?)
    }

    fn query(&mut self, sql: &str, args: &[Value]) -> Result<Vec<Row>> {
        let bound: Vec<Bound<'_>> = args.iter().map(Bound).collect();
        let refs: Vec<&(dyn ToSql + Sync)> = bound.iter().map(|value| value as &(dyn ToSql + Sync)).collect();
        let rows = self.0.query(for_postgres(sql).as_str(), &refs)?;
        rows.iter()
            .map(|row| (0..row.len()).map(|index| column(row, index)).collect::<Result<Vec<Value>>>().map(Row))
            .collect()
    }

    fn batch(&mut self, sql: &str) -> Result<()> {
        Ok(self.0.batch_execute(sql)?)
    }
}
