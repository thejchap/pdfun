//! Pluggable URL fetching for `<img>`, `background-image`, and `@font-face`.
//!
//! Mirrors `WeasyPrint`'s `url_fetcher` callback: server callers (Django,
//! Axum, etc.) need to plug in their own connection pool, auth, retries,
//! tracing, cookies. We expose a `UrlFetcher` trait, ship a default that
//! is file-only, and ship an opt-in HTTP impl behind the `http-fetch`
//! Cargo feature.

/// Simple url-scheme classifier so dispatch logic doesn't have to parse
/// the URL twice. Anything we don't recognize is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Variants populated as later rungs wire in dispatch.
pub enum Scheme {
    Http,
    Https,
    File,
    Data,
    Other,
}

/// Pure-function scheme classifier — no I/O, just a prefix match.
/// Case-insensitive per RFC 3986 §3.1.
#[allow(dead_code)] // Wired into dispatch by later rungs.
pub fn parse_scheme(url: &str) -> Scheme {
    // Find the first ':' before any character that's not a valid
    // scheme char — that's the scheme delimiter.
    let mut scheme_end = None;
    for (i, c) in url.char_indices() {
        if c == ':' {
            scheme_end = Some(i);
            break;
        }
        // Per RFC 3986, scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ).
        // Anything else first means there is no scheme.
        if !(c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
            return Scheme::Other;
        }
    }
    let Some(end) = scheme_end else {
        return Scheme::Other;
    };
    let lower = url[..end].to_ascii_lowercase();
    match lower.as_str() {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        "file" => Scheme::File,
        "data" => Scheme::Data,
        _ => Scheme::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_http_scheme() {
        assert_eq!(parse_scheme("http://example.com/x"), Scheme::Http);
        assert_eq!(parse_scheme("HTTP://EXAMPLE.COM"), Scheme::Http);
    }

    #[test]
    fn detects_https_scheme() {
        assert_eq!(parse_scheme("https://example.com/x"), Scheme::Https);
        assert_eq!(parse_scheme("HTTPS://EXAMPLE.COM"), Scheme::Https);
    }

    #[test]
    fn detects_file_scheme() {
        assert_eq!(parse_scheme("file:///etc/passwd"), Scheme::File);
    }

    #[test]
    fn detects_data_scheme() {
        assert_eq!(parse_scheme("data:text/plain;base64,QQ=="), Scheme::Data);
    }

    #[test]
    fn unknown_or_relative_is_other() {
        assert_eq!(parse_scheme("relative/path.png"), Scheme::Other);
        assert_eq!(parse_scheme("/absolute/path.png"), Scheme::Other);
        assert_eq!(parse_scheme("ftp://x"), Scheme::Other);
    }
}
