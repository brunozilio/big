mod board;
mod follow;
mod logging;
mod teammate;

use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = brand::COMMAND, version, about = concat!("Start ", brand::name!(), " on this computer and open it in the browser."))]
struct Cli {
    #[arg(long, default_value_t = 0, help = "The port to listen on (any free one by default)")]
    port: u16,
    #[arg(long = "no-open", help = "Only print the address; do not open the browser")]
    no_open: bool,
    #[command(subcommand)]
    command: Option<Internal>,
}

#[derive(Debug, Subcommand)]
enum Internal {
    #[command(hide = true, about = "Run as a teammate of a lead's team (started by the lead)")]
    Teammate(teammate::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::install();
    let ended = match cli.command {
        Some(Internal::Teammate(args)) => teammate::run(args),
        None => serve(cli.port, !cli.no_open),
    };
    match ended {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{brand_command}: {message}", brand_command = brand::COMMAND);
            ExitCode::FAILURE
        }
    }
}

fn serve(port: u16, open: bool) -> Result<ExitCode, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let cwd = std::fs::canonicalize(&cwd).map_err(|error| format!("{}: {error}", cwd.display()))?;
    let client = app::Client::builder().cwd(&cwd).build().map_err(|error| error.to_string())?;
    let running =
        console::start(console::Options { assets: console::discover(), port }).map_err(|error| error.to_string())?;
    let live = running.live();
    live.desk().open(Arc::new(console::SessionChats::new(client.clone(), cwd.clone(), live.clone())));
    let url = running.url();
    println!("{} is running at http://{}", brand::NAME, running.address());
    println!("Open it with: {url}");
    if open {
        open_in_browser(&url);
    }
    follow::run(&client, &live, &cwd);
    Ok(ExitCode::SUCCESS)
}

fn open_in_browser(url: &str) {
    let mut command = platform::open_url(url);
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    match command.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(error) => tracing::warn!(%error, "the browser could not be opened"),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn the_launcher_takes_only_a_port_and_whether_to_open_the_browser() {
        let cli = Cli::parse_from([brand::COMMAND, "--port", "4321", "--no-open"]);
        assert_eq!((cli.port, cli.no_open), (4321, true));
        assert!(cli.command.is_none());
        let plain = Cli::parse_from([brand::COMMAND]);
        assert_eq!((plain.port, plain.no_open), (0, false));
        assert!(Cli::try_parse_from([brand::COMMAND, "--print", "hi"]).is_err());
        assert!(Cli::try_parse_from([brand::COMMAND, "login"]).is_err());
    }
}
