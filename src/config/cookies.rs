use std::fs;
use std::io::Write;
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CookiesError {
    #[error("cookies file not found: {0} (run `termtube login`)")]
    NotFound(String),
    #[error("failed to read or write cookies file: {0}")]
    Io(#[from] std::io::Error),
    #[error("cookies file has no YouTube session (missing SAPISID); run `termtube login`")]
    NoSession,
}

const NETSCAPE_HEADER: &str = "# Netscape HTTP Cookie File";

/// One cookie entry from a Netscape `cookies.txt` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub domain: String,
    pub http_only: bool,
    pub include_subdomains: bool,
    pub path: String,
    pub secure: bool,
    pub expires: u64,
    pub name: String,
    pub value: String,
}

impl Cookie {
    /// Formats the cookie back into a Netscape line.
    fn to_line(&self) -> String {
        let flag = |b: bool| if b { "TRUE" } else { "FALSE" };
        format!(
            "{}{}\t{}\t{}\t{}\t{}\t{}\t{}",
            if self.http_only { "#HttpOnly_" } else { "" },
            self.domain,
            flag(self.include_subdomains),
            self.path,
            flag(self.secure),
            self.expires,
            self.name,
            self.value
        )
    }
}

/// Parses a Netscape cookies file, skipping comments and malformed lines.
pub fn parse_netscape(content: &str) -> Vec<Cookie> {
    content
        .lines()
        .filter_map(|line| {
            let (http_only, line) = match line.strip_prefix("#HttpOnly_") {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            if line.starts_with('#') || line.trim().is_empty() {
                return None;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Cookie {
                domain: f[0].to_string(),
                http_only,
                include_subdomains: f[1].eq_ignore_ascii_case("TRUE"),
                path: f[2].to_string(),
                secure: f[3].eq_ignore_ascii_case("TRUE"),
                expires: f[4].parse().unwrap_or(0),
                name: f[5].to_string(),
                value: f[6].to_string(),
            })
        })
        .collect()
}

/// Keeps only cookies scoped to youtube.com and its subdomains.
pub fn youtube_only(cookies: Vec<Cookie>) -> Vec<Cookie> {
    cookies
        .into_iter()
        .filter(|c| {
            let d = c.domain.trim_start_matches('.');
            d == "youtube.com" || d.ends_with(".youtube.com")
        })
        .collect()
}

/// Builds the value of an HTTP `Cookie` header.
pub fn to_header(cookies: &[Cookie]) -> String {
    cookies
        .iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Returns true when the cookies contain an authenticated YouTube session.
pub fn has_session(cookies: &[Cookie]) -> bool {
    cookies
        .iter()
        .any(|c| c.name == "SAPISID" && !c.value.is_empty())
}

/// Writes cookies as a Netscape file readable only by the current user.
pub fn write_netscape(path: &Path, cookies: &[Cookie]) -> Result<(), CookiesError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    writeln!(file, "{NETSCAPE_HEADER}")?;
    for c in cookies {
        writeln!(file, "{}", c.to_line())?;
    }
    Ok(())
}

/// Loads the session cookies file and returns the `Cookie` header value.
pub fn load_header(path: &Path) -> Result<String, CookiesError> {
    if !path.exists() {
        return Err(CookiesError::NotFound(path.display().to_string()));
    }
    let cookies = youtube_only(parse_netscape(&fs::read_to_string(path)?));
    if !has_session(&cookies) {
        return Err(CookiesError::NoSession);
    }
    Ok(to_header(&cookies))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Netscape HTTP Cookie File\n\
        .youtube.com\tTRUE\t/\tTRUE\t1900000000\tSAPISID\tabc123/XYZ\n\
        #HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t1900000000\tHSID\thsidvalue\n\
        .example.com\tTRUE\t/\tFALSE\t1900000000\tother\tnope\n\
        music.youtube.com\tFALSE\t/\tTRUE\t0\tPREF\tf6=40000000\n\
        malformed line\n";

    #[test]
    fn test_parse_netscape_reads_valid_lines_and_httponly() {
        let c = parse_netscape(SAMPLE);
        assert_eq!(c.len(), 4);
        assert_eq!(c[0].name, "SAPISID");
        assert!(!c[0].http_only);
        assert!(c[1].http_only);
        assert_eq!(c[1].domain, ".youtube.com");
        assert_eq!(c[3].value, "f6=40000000");
    }

    #[test]
    fn test_youtube_only_drops_other_domains() {
        let c = youtube_only(parse_netscape(SAMPLE));
        assert_eq!(c.len(), 3);
        assert!(c.iter().all(|c| c.domain.contains("youtube.com")));
    }

    #[test]
    fn test_to_header_joins_name_value_pairs() {
        let c = youtube_only(parse_netscape(SAMPLE));
        assert_eq!(
            to_header(&c),
            "SAPISID=abc123/XYZ; HSID=hsidvalue; PREF=f6=40000000"
        );
    }

    #[test]
    fn test_has_session_requires_sapisid() {
        assert!(has_session(&parse_netscape(SAMPLE)));
        assert!(!has_session(&parse_netscape(
            ".youtube.com\tTRUE\t/\tTRUE\t0\tHSID\tx\n"
        )));
    }

    #[test]
    fn test_write_then_load_roundtrip_with_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cookies.txt");
        write_netscape(&path, &youtube_only(parse_netscape(SAMPLE))).unwrap();
        assert!(load_header(&path)
            .unwrap()
            .starts_with("SAPISID=abc123/XYZ"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn test_load_header_errors() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            load_header(&dir.path().join("x")),
            Err(CookiesError::NotFound(_))
        ));
        let path = dir.path().join("c.txt");
        fs::write(&path, ".youtube.com\tTRUE\t/\tTRUE\t0\tHSID\tx\n").unwrap();
        assert!(matches!(load_header(&path), Err(CookiesError::NoSession)));
    }
}
