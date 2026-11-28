use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(content) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: conformance <content folder>");
        return ExitCode::FAILURE;
    };
    let report = connectors::conformance::run(&content);
    for finding in &report.findings {
        println!("finding\t{}", finding.replace(['\n', '\t'], " "));
    }
    for id in &report.ids {
        println!("id\t{id}");
    }
    ExitCode::SUCCESS
}
