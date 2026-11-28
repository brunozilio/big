use std::collections::BTreeSet;
use std::path::Path;

fn published(root: &Path, ids: BTreeSet<String>) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut current = ids;
    current.extend(crate::routes::routes(root)?);
    Ok(current)
}

pub fn findings(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let ran = crate::harness::ran(root)?;
    let current = published(root, ran.ids)?;
    let mut found = ran.findings;
    found.extend(crate::published::findings(root, &current)?);
    Ok(found)
}

pub fn run(root: &Path, publish: bool) -> Result<(), Box<dyn std::error::Error>> {
    if publish {
        let current = published(root, crate::harness::ran(root)?.ids)?;
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
