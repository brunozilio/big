use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Ran {
    pub findings: Vec<String>,
    pub ids: BTreeSet<String>,
}

pub fn parsed(text: &str) -> Ran {
    let mut ran = Ran::default();
    for line in text.lines() {
        match line.split_once('\t') {
            Some(("finding", finding)) => ran.findings.push(finding.to_owned()),
            Some(("id", id)) => {
                ran.ids.insert(id.to_owned());
            }
            _ => {}
        }
    }
    ran
}

fn harness(root: &Path) -> Result<Ran, Box<dyn std::error::Error>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .current_dir(root)
        .args(["run", "--quiet", "-p", "connectors", "--bin", "conformance", "--"])
        .arg(root.join("content"))
        .output()?;
    if !output.status.success() {
        return Err(format!("the conformance harness did not run: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(parsed(&String::from_utf8_lossy(&output.stdout)))
}

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let ran = harness(root)?;
    let mut current = ran.ids;
    current.extend(crate::published::routes(root)?);
    let mut found = ran.findings;
    found.extend(crate::published::findings(root, &current)?);
    Ok(found)
}

pub fn run(root: &Path, publish: bool) -> Result<(), Box<dyn std::error::Error>> {
    if publish {
        let mut current = harness(root)?.ids;
        current.extend(crate::published::routes(root)?);
        let added = crate::published::publish(root, &current)?;
        println!("conformance: {added} new published id(s) in {}", crate::published::LEDGER);
    }
    let found = findings(root)?;
    for finding in &found {
        eprintln!("{finding}");
    }
    if found.is_empty() {
        println!("conformance: ok");
        Ok(())
    } else {
        Err(format!("{} conformance finding(s)", found.len()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_output_reads_as_findings_and_ids() {
        let ran =
            parsed("finding\tconformance connectors/x: a.b has no case\nid\tconnector:x\nid\taction:x/a.b\nnoise\n");
        assert_eq!(ran.findings, ["conformance connectors/x: a.b has no case"]);
        assert_eq!(ran.ids.into_iter().collect::<Vec<_>>(), ["action:x/a.b", "connector:x"]);
    }
}
