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
    /// Sign in: import your YouTube Music session from a browser, opening
    /// music.youtube.com to sign in when no session is found
    Login {
        /// Browser to read (firefox, chrome, chromium, brave, edge, safari, ...) or
        /// `browser:/path/to/profile`; by default every detected browser is tried
        #[arg(long)]
        browser: Option<String>,
        /// Account index when several Google accounts are signed in
        #[arg(long)]
        account: Option<u8>,
        /// Import a cookies.txt (Netscape format) exported from the browser instead
        #[arg(long, value_name = "FILE")]
        cookies_file: Option<PathBuf>,
        /// Fail instead of opening music.youtube.com when no session is found
        #[arg(long)]
        no_open: bool,
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
        Some(Command::Login {
            browser,
            account,
            cookies_file,
            no_open,
        }) => {
            let opts = login::LoginOptions {
                browser,
                account,
                cookies_file,
                no_open,
            };
            login::login(&mut settings, &config_path, opts)
                .await
                .map(|()| println!("Run `termtube` to start listening."))
                .map_err(|e| e.to_string())
        }
        None => run(settings, &config_path).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("termtube: {e}");
            wait_before_closing();
            ExitCode::FAILURE
        }
    }
}

/// Keeps the window open when launched from a desktop icon, so errors can be read.
fn wait_before_closing() {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        eprintln!("Press Enter to close.");
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }
}

/// Starts the player, signing in first when there is no valid session.
async fn run(mut settings: Settings, config_path: &std::path::Path) -> Result<(), String> {
    let ytm = match connect(&settings).await {
        Ok(ytm) => ytm,
        Err(reason) => {
            println!("{reason}. Let's sign in to YouTube Music.\n");
            login::login(&mut settings, config_path, login::LoginOptions::default())
                .await
                .map_err(|e| e.to_string())?;
            connect(&settings).await?
        }
    };
    app::run::run(settings, ytm)
        .await
        .map_err(|e| e.to_string())
}

/// Connects with the saved cookies and checks that the session is still signed in.
async fn connect(settings: &Settings) -> Result<ytm::Ytm, String> {
    let header = cookies::load_header(&settings.cookies_path()).map_err(|e| match e {
        cookies::CookiesError::NotFound(_) => "No YouTube Music session yet".to_string(),
        cookies::CookiesError::NoSession => "The saved session is not signed in".to_string(),
        other => other.to_string(),
    })?;
    eprintln!("Connecting to YouTube Music…");
    let ytm = ytm::Ytm::connect(header, settings.account.authuser)
        .await
        .map_err(|e| e.to_string())?;
    match ytm.accounts().await {
        Ok(accounts) if !accounts.is_empty() => Ok(ytm),
        Ok(_) => Err("Your YouTube Music session has expired".to_string()),
        Err(e) => Err(e.to_string()),
    }
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
