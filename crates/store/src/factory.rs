use database::args;
use domain::factory::{Finding, FindingKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::log::now;
use crate::schema::next_seq;
use crate::{Result, Store};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Generation {
    pub version: String,
    pub chain: Vec<String>,
    pub files: Value,
    pub first_seen: i64,
}

impl Store {
    pub fn record_findings(&self, findings: &[Finding]) -> Result<Vec<Finding>> {
        let insert = format!(
            "INSERT INTO factory_findings (id, item, workflow, generation, kind, step, data, at, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, {}) ON CONFLICT (id) DO NOTHING",
            next_seq("factory_findings")
        );
        self.write(|conn| {
            let mut fresh = Vec::new();
            for finding in findings {
                let inserted = conn.execute(
                    &insert,
                    args![
                        finding.id,
                        finding.item.as_str(),
                        finding.workflow,
                        finding.generation,
                        finding.kind.as_str(),
                        finding.step,
                        serde_json::to_string(finding)?,
                        finding.at,
                    ],
                )?;
                if inserted > 0 {
                    fresh.push(finding.clone());
                }
            }
            Ok(fresh)
        })
    }

    pub fn findings(&self, kind: Option<FindingKind>) -> Result<Vec<Finding>> {
        let texts: Vec<String> = self.read(|conn| {
            Ok(match kind {
                Some(kind) => conn.all(
                    "SELECT data FROM factory_findings WHERE kind = ?1 ORDER BY at, seq, id",
                    args![kind.as_str()],
                )?,
                None => conn.all("SELECT data FROM factory_findings ORDER BY at, seq, id", &[])?,
            })
        })?;
        Ok(texts.iter().map(|text| serde_json::from_str(text)).collect::<serde_json::Result<_>>()?)
    }

    pub fn record_generation(&self, version: &str, chain: &[String], files: &Value) -> Result<bool> {
        let chain = serde_json::to_string(chain)?;
        let files = serde_json::to_string(files)?;
        self.write(|conn| {
            let inserted = conn.execute(
                "INSERT INTO factory_generations (version, chain, files, first_seen) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (version) DO NOTHING",
                args![version, chain, files, now()],
            )?;
            Ok(inserted > 0)
        })
    }

    pub fn generation(&self, version: &str) -> Result<Option<Generation>> {
        let row = self.read(|conn| {
            let row = conn
                .row("SELECT chain, files, first_seen FROM factory_generations WHERE version = ?1", args![version])?;
            Ok(row
                .map(|row| Ok::<_, database::Error>((row.get::<String>(0)?, row.get::<String>(1)?, row.get(2)?)))
                .transpose()?)
        })?;
        row.map(|(chain, files, first_seen)| {
            Ok(Generation {
                version: version.to_owned(),
                chain: serde_json::from_str(&chain)?,
                files: serde_json::from_str(&files)?,
                first_seen,
            })
        })
        .transpose()
    }
}
