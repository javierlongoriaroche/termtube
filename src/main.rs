mod app;
mod config;
mod login;
mod player;
mod queue;
mod ui;
mod ytm;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;

use clap::{Parser, Subcommand};

use config::cookies;
use config::settings::{default_config_path, Settings};

#[derive(Parser)]
#[command(name = "termtube", version, about = "YouTube Music in your terminal")]
struct Cli {
    /// Path to config.toml
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Import your YouTube Music session from a browser
    Login {
        /// Browser to read cookies from (brave, chrome, firefox, safari, ...)
        #[arg(long)]
        browser: Option<String>,
        /// Account index when several Google accounts are signed in
        #[arg(long)]
        account: Option<u8>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(default_config_path);
    let mut settings = match Settings::load_or_default(&config_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid config {}: {e}", config_path.display());
            return ExitCode::FAILURE;
        }
    };
    init_logging(&settings);

    let result = match cli.command {
        Some(Command::Login { browser, account }) => {
            login::login(&mut settings, &config_path, browser, account)
                .await
                .map_err(|e| e.to_string())
        }
        None => run(settings).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("termtube: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(settings: Settings) -> Result<(), String> {
    let header = cookies::load_header(&settings.cookies_path()).map_err(|e| e.to_string())?;
    eprintln!("Connecting to YouTube Music…");
    let ytm = ytm::Ytm::connect(header, settings.account.authuser)
        .await
        .map_err(|e| e.to_string())?;
    app::run::run(settings, ytm)
        .await
        .map_err(|e| e.to_string())
}

/// Logs to `<data_dir>/termtube.log`; the terminal belongs to the UI.
fn init_logging(settings: &Settings) {
    let path = settings.log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "termtube=info".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init();
}
