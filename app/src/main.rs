#![forbid(unsafe_code)]

use std::{error::Error, io, process::ExitCode};

use clap::{Parser, Subcommand};
use loncher_domain::{DaemonCommand, DaemonReply};
use loncher_runtime::{dispatch_command, run_daemon};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "loncher", version, about = "Linux/Niri-first desktop control plane")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the long-lived user daemon.
    Daemon,
    /// Show the launcher surface.
    Show {
        #[arg(long)]
        query: Option<String>,
    },
    /// Hide the launcher surface.
    Hide,
    /// Toggle the launcher surface.
    Toggle {
        #[arg(long)]
        query: Option<String>,
    },
    /// Open the launcher with a search query.
    Query { text: String },
    /// Open an agent session with an optional initial prompt.
    Agent { prompt: Option<String> },
    /// Query daemon status as JSON.
    Status,
    /// Gracefully stop the running daemon.
    Shutdown,
}

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error + Send + Sync>> {
    let dotenv_result = dotenvy::dotenv();
    init_tracing()?;
    report_dotenv_result(dotenv_result);

    let cli = Cli::parse();

    match cli.command {
        Command::Daemon => run_daemon_process().await?,
        Command::Show { query } => {
            dispatch_and_report(DaemonCommand::Show { query }, false).await?;
        }
        Command::Hide => dispatch_and_report(DaemonCommand::Hide, false).await?,
        Command::Toggle { query } => {
            dispatch_and_report(DaemonCommand::Toggle { query }, false).await?;
        }
        Command::Query { text } => {
            dispatch_and_report(DaemonCommand::Query { text }, false).await?;
        }
        Command::Agent { prompt } => {
            dispatch_and_report(DaemonCommand::OpenAgent { prompt }, false).await?;
        }
        Command::Status => dispatch_and_report(DaemonCommand::Status, true).await?,
        Command::Shutdown => dispatch_and_report(DaemonCommand::Shutdown, false).await?,
    }

    Ok(ExitCode::SUCCESS)
}

async fn dispatch_and_report(
    command: DaemonCommand,
    print_reply: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let reply = dispatch_command(command).await?;
    if print_reply {
        let snapshot = match reply {
            DaemonReply::Accepted { snapshot } | DaemonReply::Status { snapshot } => snapshot,
        };
        println!("{}", serde_json::to_string(&snapshot)?);
    }
    Ok(())
}

fn init_tracing() -> Result<(), Box<dyn Error + Send + Sync>> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(error) => {
            eprintln!("invalid RUST_LOG, falling back to info: {error}");
            EnvFilter::new("info")
        }
    };

    tracing_subscriber::fmt().with_env_filter(filter).try_init()?;
    Ok(())
}

fn report_dotenv_result(result: Result<std::path::PathBuf, dotenvy::Error>) {
    match result {
        Ok(path) => debug!(?path, "loaded dotenv file"),
        Err(dotenvy::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => warn!(%error, "failed to load dotenv file"),
    }
}

async fn run_daemon_process() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cancellation = CancellationToken::new();
    let daemon_cancellation = cancellation.child_token();
    let mut daemon = tokio::spawn(async move { run_daemon(daemon_cancellation).await });

    info!("daemon started");

    tokio::select! {
        result = &mut daemon => {
            result??;
            info!("daemon stopped");
            return Ok(());
        }
        signal_name = shutdown_signal() => {
            let signal_name = signal_name?;
            warn!(signal = signal_name, "shutdown signal received");
        }
    }

    cancellation.cancel();
    daemon.await??;
    info!("daemon stopped");
    Ok(())
}

async fn shutdown_signal() -> io::Result<&'static str> {
    let mut terminate = signal(SignalKind::terminate())?;

    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            Ok("SIGINT")
        }
        received = terminate.recv() => {
            match received {
                Some(()) => Ok("SIGTERM"),
                None => Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "SIGTERM signal stream closed",
                )),
            }
        }
    }
}
