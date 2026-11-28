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

pub fn ran(root: &Path) -> Result<Ran, Box<dyn std::error::Error>> {
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
