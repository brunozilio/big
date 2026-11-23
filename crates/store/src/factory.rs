use domain::factory::{Finding, FindingKind};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::log::now;
use crate::{Result, Store, unsigned};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Generation {
    pub version: String,
    pub chain: Vec<String>,
    pub files: Value,
    pub first_seen: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationReport {
    pub version: Option<String>,
    pub items: u64,
    pub finished: u64,
    pub failures: u64,
    pub escapes: u64,
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

    pub fn count_findings(&self, workflow: &str, step: &str, kind: FindingKind) -> Result<u64> {
        let count: i64 = self.locked().query_row(
            "SELECT COUNT(*) FROM factory_findings WHERE workflow = ?1 AND step = ?2 AND kind = ?3",
            params![workflow, step, kind.as_str()],
            |row| row.get(0),
        )?;
        unsigned(count)
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

    pub fn generations(&self) -> Result<Vec<GenerationReport>> {
        let connection = self.locked();
        let mut statement = connection.prepare(
            "SELECT g.version,
                (SELECT COUNT(*) FROM items i WHERE json_extract(i.state, '$.input.generation.version') = g.version),
                (SELECT COUNT(*) FROM items i WHERE json_extract(i.state, '$.input.generation.version') = g.version
                    AND json_extract(i.state, '$.ending.type') = 'finished'),
                (SELECT COUNT(*) FROM factory_findings f WHERE f.generation = g.version AND f.kind = 'failure'),
                (SELECT COUNT(*) FROM factory_findings f WHERE f.generation = g.version AND f.kind = 'escape')
             FROM factory_generations g ORDER BY g.first_seen, g.rowid",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(version, items, finished, failures, escapes)| {
                Ok(GenerationReport {
                    version: Some(version),
                    items: unsigned(items)?,
                    finished: unsigned(finished)?,
                    failures: unsigned(failures)?,
                    escapes: unsigned(escapes)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use domain::factory::{Finding, FindingKind};
    use domain::workflow::{ItemId, WorkflowEvent};
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
        assert_eq!(store.count_findings("entrega", "checks", FindingKind::Failure).expect("count"), 1);
        assert_eq!(store.findings(Some(FindingKind::Escape)).expect("list").len(), 1);
        assert_eq!(store.findings(None).expect("list").len(), 2);
    }

    #[test]
    fn each_generation_reports_its_items_and_findings() {
        let store = Store::in_memory(0).expect("store");
        assert!(store.record_generation("g1", &["base".into()], &json!({"agents/a.md": "h"})).expect("record"));
        assert!(!store.record_generation("g1", &[], &json!({})).expect("record"), "a version is immutable");
        store.record_generation("g2", &[], &json!({})).expect("record");
        let created = WorkflowEvent::ItemCreated {
            id: ItemId::from("i1"),
            workflow: "entrega".into(),
            parent: None,
            input: json!({ "generation": { "version": "g1" } }),
        };
        store.append(&ItemId::from("i1"), &[created, WorkflowEvent::Finished { output: json!(null) }]).expect("append");
        store
            .record_findings(&[finding("a", FindingKind::Failure, "g1"), finding("b", FindingKind::Escape, "g2")])
            .expect("record");
        let reports = store.generations().expect("reports");
        assert_eq!(reports.len(), 2);
        assert_eq!((reports[0].items, reports[0].finished, reports[0].failures, reports[0].escapes), (1, 1, 1, 0));
        assert_eq!((reports[1].items, reports[1].escapes), (0, 1));
        assert_eq!(store.generation("g1").expect("load").expect("found").chain, vec!["base".to_owned()]);
    }
}
