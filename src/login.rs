use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::process::Command;

use crate::config::cookies::{self, Cookie, CookiesError};
use crate::config::settings::{Settings, SettingsError};
use crate::ytm::{Ytm, YtmError};

/// Highest `X-Goog-AuthUser` index probed when listing accounts.
const MAX_ACCOUNTS: u8 = 5;
const SIGN_IN_URL: &str = "https://music.youtube.com";
/// How long to wait for the user to sign in after opening the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// yt-dlp exports the browser cookies even when the URL is not supported,
/// so a placeholder avoids any network request while importing them.
const PLACEHOLDER_URL: &str = "termtube:cookie-export";

#[derive(Debug, Error)]
pub enum LoginError {
    #[error("could not run yt-dlp ({0}); install it (macOS: `brew install yt-dlp`)")]
    YtDlp(std::io::Error),
    #[error("no supported browser found; use `termtube login --browser <name>` or `--cookies-file <file>`")]
    NoBrowser,
    #[error(
        "no YouTube Music session found in {0}; sign in at https://music.youtube.com and retry"
    )]
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

/// How `termtube login` should obtain the session.
#[derive(Debug, Clone, Default)]
pub struct LoginOptions {
    /// Browser (or `browser:profile-path`) to read; all detected browsers when `None`.
    pub browser: Option<String>,
    pub account: Option<u8>,
    /// Netscape cookies file exported manually, instead of reading a browser.
    pub cookies_file: Option<PathBuf>,
    /// Do not open music.youtube.com when no session is found.
    pub no_open: bool,
}

/// A browser profile location readable by yt-dlp's `--cookies-from-browser`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub label: String,
    pub spec: String,
}

/// Removes the temporary full-browser cookie export when dropped.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Imports the YouTube session and selects the account to use.
pub async fn login(
    settings: &mut Settings,
    config_path: &Path,
    opts: LoginOptions,
) -> Result<(), LoginError> {
    std::fs::create_dir_all(settings.data_dir())?;
    let (session, source) = match &opts.cookies_file {
        Some(file) => {
            let session =
                cookies::youtube_only(cookies::parse_netscape(&std::fs::read_to_string(file)?));
            if !cookies::has_session(&session) {
                return Err(LoginError::NoSession(file.display().to_string()));
            }
            (session, None)
        }
        None => {
            let (session, source) = find_session(settings, &opts).await?;
            (session, Some(source))
        }
    };
    cookies::write_netscape(&settings.cookies_path(), &session)?;
    println!(
        "Saved {} YouTube cookies to {}",
        session.len(),
        settings.cookies_path().display()
    );

    let header = cookies::to_header(&session);
    let accounts = list_accounts(&header).await?;
    let chosen = match (opts.account, accounts.len()) {
        (Some(n), len) if (n as usize) < len => n,
        (Some(n), len) => return Err(LoginError::BadAccount(n, len)),
        (None, 0 | 1) => 0,
        (None, _) => prompt_account(&accounts)?,
    };

    if let Some(source) = source {
        settings.account.browser = source.spec;
    }
    settings.account.authuser = chosen;
    settings.save(config_path)?;

    let ytm = Ytm::connect(header, chosen).await?;
    let playlists = ytm.library_playlists().await?.len().saturating_sub(1);
    let name = accounts
        .get(chosen as usize)
        .map(String::as_str)
        .unwrap_or("default account");
    println!("Logged in as {name} ({playlists} playlists in your library).");
    Ok(())
}

/// Looks for a signed-in browser; if none, opens YouTube Music and waits for the sign-in.
async fn find_session(
    settings: &Settings,
    opts: &LoginOptions,
) -> Result<(Vec<Cookie>, Source), LoginError> {
    let sources = match &opts.browser {
        Some(spec) => vec![Source {
            label: spec.clone(),
            spec: spec.clone(),
        }],
        None => {
            let home = dirs::home_dir().unwrap_or_default();
            prefer(
                detect_sources(&home, cfg!(target_os = "macos")),
                &settings.account.browser,
            )
        }
    };
    if sources.is_empty() {
        return Err(LoginError::NoBrowser);
    }
    let labels = sources
        .iter()
        .map(|s| s.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!("Looking for a YouTube Music session in: {labels}");
    if cfg!(target_os = "macos") {
        println!("(macOS may ask for Keychain access to read browser cookies)");
    }
    if let Some(found) = try_sources(settings, &sources).await? {
        return Ok(found);
    }
    if opts.no_open {
        return Err(LoginError::NoSession(labels));
    }

    println!("\nNo YouTube Music session found. Sign in at {SIGN_IN_URL} with the account you want to use.");
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        print!("Press Enter to open it in your browser… ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        open_url(SIGN_IN_URL);
    }
    println!(
        "Waiting for the sign-in (up to {} min, Ctrl-C to cancel)…",
        SIGN_IN_TIMEOUT.as_secs() / 60
    );
    let started = Instant::now();
    let mut hinted = false;
    while started.elapsed() < SIGN_IN_TIMEOUT {
        tokio::time::sleep(POLL_INTERVAL).await;
        if let Some(found) = try_sources(settings, &sources).await? {
            return Ok(found);
        }
        if !hinted && started.elapsed() > Duration::from_secs(60) {
            println!("Still waiting. If you already signed in, close the browser so it saves its cookies to disk.");
            hinted = true;
        }
    }
    Err(LoginError::NoSession(labels))
}

/// Returns the YouTube cookies of the first source that has a signed-in session.
async fn try_sources(
    settings: &Settings,
    sources: &[Source],
) -> Result<Option<(Vec<Cookie>, Source)>, LoginError> {
    for source in sources {
        match export_cookies(settings, source).await {
            Ok(session) if cookies::has_session(&session) => {
                println!("✓ Found a YouTube Music session in {}", source.label);
                return Ok(Some((session, source.clone())));
            }
            Ok(_) => tracing::info!("no YouTube session in {}", source.label),
            Err(LoginError::YtDlp(e)) => return Err(LoginError::YtDlp(e)),
            Err(e) => tracing::info!("could not read {}: {e}", source.label),
        }
    }
    Ok(None)
}

/// Exports a browser's cookies with yt-dlp and keeps only the youtube.com ones.
async fn export_cookies(settings: &Settings, source: &Source) -> Result<Vec<Cookie>, LoginError> {
    let export = TempFile(settings.data_dir().join(".cookies-export.txt"));
    cookies::write_netscape(&export.0, &[])?;
    let out = Command::new(&settings.mpv.ytdlp_path)
        .args([
            "--quiet",
            "--no-warnings",
            "--simulate",
            "--cookies-from-browser",
            &source.spec,
            "--cookies",
        ])
        .arg(&export.0)
        .arg(PLACEHOLDER_URL)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(LoginError::YtDlp)?;
    let session = cookies::youtube_only(cookies::parse_netscape(&std::fs::read_to_string(
        &export.0,
    )?));
    if session.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr);
        tracing::info!(
            "yt-dlp for {}: {}",
            source.spec,
            err.lines().last().unwrap_or("no cookies")
        );
    }
    Ok(session)
}

/// Browser profiles present on this machine, including snap and flatpak installs.
pub fn detect_sources(home: &Path, macos: bool) -> Vec<Source> {
    let candidates: &[(&str, &str, &[&str])] = if macos {
        &[
            (
                "Brave",
                "brave",
                &["Library/Application Support/BraveSoftware/Brave-Browser"],
            ),
            (
                "Chrome",
                "chrome",
                &["Library/Application Support/Google/Chrome"],
            ),
            (
                "Firefox",
                "firefox",
                &["Library/Application Support/Firefox"],
            ),
            (
                "Edge",
                "edge",
                &["Library/Application Support/Microsoft Edge"],
            ),
            (
                "Chromium",
                "chromium",
                &["Library/Application Support/Chromium"],
            ),
            (
                "Vivaldi",
                "vivaldi",
                &["Library/Application Support/Vivaldi"],
            ),
            (
                "Opera",
                "opera",
                &["Library/Application Support/com.operasoftware.Opera"],
            ),
            ("Safari", "safari", &["Library/Containers/com.apple.Safari"]),
        ]
    } else {
        &[
            (
                "Firefox",
                "firefox",
                &[
                    ".mozilla/firefox",
                    "snap/firefox/common/.mozilla/firefox",
                    ".var/app/org.mozilla.firefox/.mozilla/firefox",
                ],
            ),
            (
                "Chrome",
                "chrome",
                &[
                    ".config/google-chrome",
                    ".var/app/com.google.Chrome/config/google-chrome",
                ],
            ),
            (
                "Chromium",
                "chromium",
                &[
                    ".config/chromium",
                    "snap/chromium/common/chromium",
                    ".var/app/org.chromium.Chromium/config/chromium",
                ],
            ),
            (
                "Brave",
                "brave",
                &[
                    ".config/BraveSoftware/Brave-Browser",
                    "snap/brave/current/.config/BraveSoftware/Brave-Browser",
                    ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
                ],
            ),
            ("Edge", "edge", &[".config/microsoft-edge"]),
            ("Vivaldi", "vivaldi", &[".config/vivaldi"]),
            ("Opera", "opera", &[".config/opera"]),
        ]
    };
    let mut sources = Vec::new();
    for (label, name, dirs) in candidates {
        for (i, rel) in dirs.iter().enumerate() {
            let dir = home.join(rel);
            if !dir.is_dir() {
                continue;
            }
            // The first location is the browser's default; others need an explicit profile path.
            let (label, spec) = if i == 0 {
                (label.to_string(), name.to_string())
            } else {
                let kind = if rel.starts_with("snap/") {
                    "snap"
                } else {
                    "flatpak"
                };
                (
                    format!("{label} ({kind})"),
                    format!("{name}:{}", dir.display()),
                )
            };
            sources.push(Source { label, spec });
        }
    }
    sources
}

/// Moves the source used last time to the front.
fn prefer(mut sources: Vec<Source>, spec: &str) -> Vec<Source> {
    if let Some(i) = sources.iter().position(|s| s.spec == spec) {
        let s = sources.remove(i);
        sources.insert(0, s);
    }
    sources
}

/// Opens a URL in the default browser, ignoring failures (the URL is also printed).
fn open_url(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with(dirs: &[&str]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for d in dirs {
            std::fs::create_dir_all(home.path().join(d)).unwrap();
        }
        home
    }

    #[test]
    fn test_detect_linux_default_snap_and_flatpak_profiles() {
        let home = home_with(&[
            ".mozilla/firefox",
            "snap/chromium/common/chromium",
            ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
        ]);
        let sources = detect_sources(home.path(), false);
        let labels: Vec<_> = sources.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Firefox", "Chromium (snap)", "Brave (flatpak)"]);
        assert_eq!(sources[0].spec, "firefox");
        assert_eq!(
            sources[1].spec,
            format!(
                "chromium:{}",
                home.path().join("snap/chromium/common/chromium").display()
            )
        );
        assert!(sources[2].spec.starts_with("brave:"));
    }

    #[test]
    fn test_detect_macos_profiles_and_empty_home() {
        let home = home_with(&[
            "Library/Application Support/Google/Chrome",
            "Library/Containers/com.apple.Safari",
        ]);
        let specs: Vec<_> = detect_sources(home.path(), true)
            .into_iter()
            .map(|s| s.spec)
            .collect();
        assert_eq!(specs, ["chrome", "safari"]);
        assert!(detect_sources(home_with(&[]).path(), false).is_empty());
    }

    #[test]
    fn test_prefer_moves_last_used_source_first() {
        let s = |n: &str| Source {
            label: n.into(),
            spec: n.into(),
        };
        let ordered = prefer(vec![s("firefox"), s("chrome"), s("brave")], "brave");
        assert_eq!(
            ordered.iter().map(|s| s.spec.as_str()).collect::<Vec<_>>(),
            ["brave", "firefox", "chrome"]
        );
        let unchanged = prefer(vec![s("firefox")], "safari");
        assert_eq!(unchanged[0].spec, "firefox");
    }
}
