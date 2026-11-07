use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;
use tokio::process::Command;

use crate::config::cookies::{self, CookiesError};
use crate::config::settings::{Settings, SettingsError};
use crate::ytm::{Ytm, YtmError};

/// Highest `X-Goog-AuthUser` index probed when listing accounts.
const MAX_ACCOUNTS: u8 = 5;

#[derive(Debug, Error)]
pub enum LoginError {
    #[error("could not run yt-dlp ({0}); install it with `brew install yt-dlp`")]
    YtDlp(std::io::Error),
    #[error("no YouTube session found in {0}; sign in at https://music.youtube.com in that browser and retry")]
    NoSession(String),
    #[error("account {0} is not available (found {1})")]
    BadAccount(u8, usize),
    #[error(transparent)]
    Cookies(#[from] CookiesError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Api(#[from] YtmError),
    #[error("input error: {0}")]
    Io(#[from] std::io::Error),
}

/// Removes the temporary full-browser cookie export when dropped.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Imports the YouTube session from a browser and selects the account to use.
pub async fn login(
    settings: &mut Settings,
    config_path: &Path,
    browser: Option<String>,
    account: Option<u8>,
) -> Result<(), LoginError> {
    let browser = browser.unwrap_or_else(|| settings.account.browser.clone());
    let data_dir = settings.data_dir();
    std::fs::create_dir_all(&data_dir)?;

    println!("Reading YouTube cookies from {browser} (macOS may ask for Keychain access)…");
    let export = TempFile(data_dir.join(".cookies-export.txt"));
    cookies::write_netscape(&export.0, &[])?;
    Command::new(&settings.mpv.ytdlp_path)
        .args([
            "--quiet",
            "--no-warnings",
            "--skip-download",
            "--flat-playlist",
            "--playlist-items",
            "1",
        ])
        .args(["--cookies-from-browser", &browser, "--cookies"])
        .arg(&export.0)
        .arg("https://music.youtube.com/playlist?list=LM")
        .status()
        .await
        .map_err(LoginError::YtDlp)?;

    let session = cookies::youtube_only(cookies::parse_netscape(&std::fs::read_to_string(
        &export.0,
    )?));
    drop(export);
    if !cookies::has_session(&session) {
        return Err(LoginError::NoSession(browser));
    }
    cookies::write_netscape(&settings.cookies_path(), &session)?;
    println!(
        "Saved {} YouTube cookies to {}",
        session.len(),
        settings.cookies_path().display()
    );

    let header = cookies::to_header(&session);
    let accounts = list_accounts(&header).await?;
    let chosen = match (account, accounts.len()) {
        (Some(n), len) if (n as usize) < len => n,
        (Some(n), len) => return Err(LoginError::BadAccount(n, len)),
        (None, 0 | 1) => 0,
        (None, _) => prompt_account(&accounts)?,
    };

    settings.account.browser = browser;
    settings.account.authuser = chosen;
    settings.save(config_path)?;

    let ytm = Ytm::connect(header, chosen).await?;
    let playlists = ytm.library_playlists().await?.len().saturating_sub(1);
    let name = accounts
        .get(chosen as usize)
        .map(String::as_str)
        .unwrap_or("default account");
    println!(
        "Logged in as {name} ({playlists} playlists in your library). Run `termtube` to start."
    );
    Ok(())
}

/// Names of the accounts in the session, indexed by `X-Goog-AuthUser`.
async fn list_accounts(header: &str) -> Result<Vec<String>, LoginError> {
    let mut names: Vec<String> = Vec::new();
    for index in 0..MAX_ACCOUNTS {
        let Ok(ytm) = Ytm::connect(header.to_string(), index).await else {
            break;
        };
        let Ok(accounts) = ytm.accounts().await else {
            break;
        };
        let Some(active) = accounts.into_iter().next() else {
            break;
        };
        if names.contains(&active.name) {
            break;
        }
        names.push(active.name);
    }
    if names.is_empty() {
        Ytm::connect(header.to_string(), 0).await?;
    }
    Ok(names)
}

fn prompt_account(accounts: &[String]) -> Result<u8, LoginError> {
    println!("Several Google accounts are signed in:");
    for (i, name) in accounts.iter().enumerate() {
        println!("  [{i}] {name}");
    }
    let stdin = std::io::stdin();
    loop {
        print!("Account to use [0-{}]: ", accounts.len() - 1);
        std::io::stdout().flush()?;
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        match line.trim().parse::<u8>() {
            Ok(n) if (n as usize) < accounts.len() => return Ok(n),
            _ => println!("Please type a number from the list."),
        }
    }
}
