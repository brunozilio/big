use domain::factory::{Finding, FindingKind};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::log::now;
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
        self.write(|transaction| {
            let mut fresh = Vec::new();
            for finding in findings {
                let inserted = transaction.execute(
                    "INSERT OR IGNORE INTO factory_findings (id, item, workflow, generation, kind, step, data, at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
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
        let connection = self.locked();
        let mut statement =
            connection.prepare("SELECT data FROM factory_findings WHERE ?1 IS NULL OR kind = ?1 ORDER BY at, rowid")?;
        let texts = statement
            .query_map([kind.map(FindingKind::as_str)], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(texts.iter().map(|text| serde_json::from_str(text)).collect::<serde_json::Result<_>>()?)
    }

    pub fn record_generation(&self, version: &str, chain: &[String], files: &Value) -> Result<bool> {
        self.write(|transaction| {
            let inserted = transaction.execute(
                "INSERT OR IGNORE INTO factory_generations (version, chain, files, first_seen) VALUES (?1, ?2, ?3, ?4)",
                params![version, serde_json::to_string(chain)?, serde_json::to_string(files)?, now()],
            )?;
            Ok(inserted > 0)
        })
    }

    pub fn generation(&self, version: &str) -> Result<Option<Generation>> {
        let connection = self.locked();
        let row: Option<(String, String, i64)> = connection
            .query_row(
                "SELECT chain, files, first_seen FROM factory_generations WHERE version = ?1",
                [version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
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

#[cfg(test)]
mod tests {
    use domain::factory::{Finding, FindingKind};
    use domain::workflow::ItemId;
    use serde_json::json;

    use crate::Store;

    fn finding(id: &str, kind: FindingKind, generation: &str) -> Finding {
        Finding {
            id: id.into(),
            item: ItemId::from("i1"),
            workflow: "entrega".into(),
            generation: Some(generation.into()),
            kind,
            step: "checks".into(),
            gate: None,
            caught_by: None,
            detail: json!({}),
            at: 1,
        }
    }

    #[test]
    fn a_finding_is_recorded_once_and_only_the_new_ones_come_back() {
        let store = Store::in_memory(0).expect("store");
        let first = store.record_findings(&[finding("a", FindingKind::Failure, "g1")]).expect("record");
        assert_eq!(first.len(), 1);
        let again = store
            .record_findings(&[finding("a", FindingKind::Failure, "g1"), finding("b", FindingKind::Escape, "g1")])
            .expect("record");
        assert_eq!(again.iter().map(|found| found.id.as_str()).collect::<Vec<_>>(), vec!["b"]);
        assert_eq!(store.findings(Some(FindingKind::Failure)).expect("list").len(), 1);
        assert_eq!(store.findings(Some(FindingKind::Escape)).expect("list").len(), 1);
        assert_eq!(store.findings(None).expect("list").len(), 2);
    }

    #[test]
    fn a_generation_is_recorded_once_and_loads_back() {
        let store = Store::in_memory(0).expect("store");
        assert!(store.record_generation("g1", &["base".into()], &json!({"agents/a.md": "h"})).expect("record"));
        assert!(!store.record_generation("g1", &[], &json!({})).expect("record"), "a version is immutable");
        assert_eq!(store.generation("g1").expect("load").expect("found").chain, vec!["base".to_owned()]);
    }
}
