mod browser_mcp;
mod cloud;
mod discovered;
mod git_credential;
mod http;
mod instance;
mod logging;
mod runner;
mod teammate;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

use instance::Instance;

#[derive(Debug, Parser)]
#[command(name = brand::COMMAND, version, about = concat!("Start ", brand::name!(), " on this computer and open it in the browser."))]
struct Cli {
    #[arg(long, help = "The port to listen on (the one in config.toml by default)")]
    port: Option<u16>,
    #[arg(long = "no-open", help = "Only print the address; do not open the browser")]
    no_open: bool,
    #[command(subcommand)]
    command: Option<Internal>,
}

#[derive(Debug, Subcommand)]
enum Internal {
    #[command(hide = true, about = "Run as a teammate of a lead's team (started by the lead)")]
    Teammate(teammate::Args),
    #[command(name = server::CLOUD_CREDENTIALS_COMMAND, hide = true, about = "Print an agent's cloud credentials")]
    CloudCredentials(cloud::Args),
    #[command(name = server::GIT_CREDENTIAL_COMMAND, hide = true, about = "Answer git with an agent's repository credentials")]
    GitCredential(git_credential::Args),
    #[command(
        name = brand::browser::MCP_COMMAND,
        about = "Serve the browser tools over MCP on stdin/stdout, for another agent to use"
    )]
    BrowserMcp(browser_mcp::Args),
}

fn main() -> ExitCode {
    let root = std::env::current_dir().unwrap_or_default();
    let found = discovered::discover(&discovered::roots(brand::commands::DIRS, &root));
    let matches = discovered::injected(Cli::command(), &found).get_matches();
    if let Some((name, given)) = matches.subcommand()
        && let Some(command) = found.iter().find(|command| command.name == name)
    {
        let args = given.get_many::<OsString>(discovered::ARGS).map(|args| args.cloned().collect()).unwrap_or_default();
        if command.page {
            logging::install(false);
            return finish(runner::run(command, args, &root));
        }
        return finish(discovered::run(command, args, &root));
    }
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    logging::install(cli.command.is_none());
    finish(launched(cli, &root))
}

fn launched(cli: Cli, root: &Path) -> Result<ExitCode, String> {
    match cli.command {
        Some(Internal::Teammate(args)) => teammate::run(args),
        Some(Internal::CloudCredentials(args)) => cloud::run(args),
        Some(Internal::GitCredential(args)) => git_credential::run(args),
        Some(Internal::BrowserMcp(args)) => data_dir().and_then(|data| browser_mcp::run(args, data)),
        None => serve(root, cli.port.unwrap_or_else(configured_port), !cli.no_open && configured_open()),
    }
}

fn finish(ended: Result<ExitCode, String>) -> ExitCode {
    match ended {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{brand_command}: {message}", brand_command = brand::COMMAND);
            ExitCode::FAILURE
        }
    }
}

fn configured_port() -> u16 {
    brand::server::PORT.trim().parse().unwrap_or(0)
}

fn configured_open() -> bool {
    brand::server::OPEN_BROWSER.trim() != "false"
}

fn data_dir() -> Result<PathBuf, String> {
    platform::data_dir(brand::NAME, brand::COMMAND).ok_or_else(|| "this user has no data folder".to_owned())
}

fn serve(cwd: &Path, port: u16, open: bool) -> Result<ExitCode, String> {
    let repo = std::fs::canonicalize(cwd).map_err(|error| format!("{}: {error}", cwd.display()))?;
    let data = data_dir()?;
    let held = match instance::claim(&data).map_err(|error| format!("{}: {error}", data.display()))? {
        Instance::Ours(held) => held,
        Instance::Running(url) => return Ok(already_running(url, open)),
    };
    let listener = server::bind(port).map_err(|error| format!("could not listen on 127.0.0.1: {error}"))?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    let url = format!("http://{address}/");
    held.announce(&url).map_err(|error| error.to_string())?;
    let runtime =
        tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|error| error.to_string())?;
    println!("{} is running at {url}", brand::NAME);
    if open {
        open_in_browser(&url);
    }
    let launch = server::Launch { repo, data, listener };
    let stop = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let served = runtime.block_on(server::run(launch, stop));
    app::release_local_models();
    served?;
    drop(held);
    Ok(ExitCode::SUCCESS)
}

fn already_running(url: Option<String>, open: bool) -> ExitCode {
    match url {
        Some(url) => {
            println!("{} is already running at {url}", brand::NAME);
            if open {
                open_in_browser(&url);
            }
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("{} is already running on this computer, but its address is not known yet.", brand::NAME);
            ExitCode::FAILURE
        }
    }
}

pub fn open_in_browser(url: &str) {
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
    #![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

    use clap::Parser;

    use super::*;

    #[test]
    fn the_launcher_takes_only_a_port_and_whether_to_open_the_browser() {
        let cli = Cli::parse_from([brand::COMMAND, "--port", "4321", "--no-open"]);
        assert_eq!((cli.port, cli.no_open), (Some(4321), true));
        assert!(cli.command.is_none());
        let plain = Cli::parse_from([brand::COMMAND]);
        assert_eq!((plain.port, plain.no_open), (None, false));
        assert!(Cli::try_parse_from([brand::COMMAND, "--print", "hi"]).is_err());
        assert!(Cli::try_parse_from([brand::COMMAND, "login"]).is_err());
    }

    #[test]
    fn the_cloud_credentials_command_takes_the_address_it_asks() {
        let cli = Cli::parse_from([brand::COMMAND, server::CLOUD_CREDENTIALS_COMMAND, "http://127.0.0.1:1/x"]);
        assert!(matches!(cli.command, Some(Internal::CloudCredentials(_))));
    }

    #[test]
    fn the_browser_mcp_command_takes_no_arguments() {
        let cli = Cli::parse_from([brand::COMMAND, brand::browser::MCP_COMMAND]);
        assert!(matches!(cli.command, Some(Internal::BrowserMcp(_))));
    }

    #[test]
    fn a_discovered_command_joins_the_launcher_and_keeps_its_arguments() {
        let found = vec![discovered::Discovered {
            name: "ship-it".into(),
            about: "Ship".into(),
            run: vec!["true".into()],
            folder: PathBuf::from("."),
            page: false,
        }];
        let cli = discovered::injected(Cli::command(), &found);
        let matches = cli.try_get_matches_from([brand::COMMAND, "ship-it", "--dry", "x"]).expect("parsed");
        let (name, given) = matches.subcommand().expect("subcommand");
        let args: Vec<&OsString> = given.get_many::<OsString>(discovered::ARGS).expect("args").collect();
        assert_eq!((name, args.len()), ("ship-it", 2));
        let clash = vec![discovered::Discovered { name: server::GIT_CREDENTIAL_COMMAND.into(), ..found[0].clone() }];
        let kept = discovered::injected(Cli::command(), &clash);
        assert_eq!(kept.get_subcommands().count(), Cli::command().get_subcommands().count());
    }

    #[test]
    fn the_defaults_come_from_the_configuration() {
        assert_eq!(configured_port().to_string(), brand::server::PORT.trim());
        assert_eq!(configured_open(), brand::server::OPEN_BROWSER.trim() != "false");
    }
}
