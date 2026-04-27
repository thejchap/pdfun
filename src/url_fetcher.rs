//! Pluggable URL fetching for `<img>`, `background-image`, and `@font-face`.
//!
//! Mirrors `WeasyPrint`'s `url_fetcher` callback: server callers (Django,
//! Axum, etc.) need to plug in their own connection pool, auth, retries,
//! tracing, cookies. We expose a `UrlFetcher` trait, ship a default that
//! is file-only, and ship an opt-in HTTP impl behind the `http-fetch`
//! Cargo feature.

use std::path::{Path, PathBuf};

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

/// Bytes returned by a fetch, plus the metadata downstream consumers
/// need: a content-type hint (so PNG vs JPEG vs TTF dispatch can skip
/// magic-byte sniffing in the easy cases) and the URL we ended at after
/// any redirects (so relative resources inside the resource resolve
/// correctly — same trick `curl -L` uses).
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields read by later rungs / external callers.
pub struct FetchedResource {
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
    pub redirected_url: Option<String>,
}

/// The pluggable interface. Implementations must be `Send + Sync` so
/// `Arc<dyn UrlFetcher>` can be shared across renders. The `base_dir`
/// parameter is passed through for relative-path resolution; HTTP
/// implementations should ignore it.
pub trait UrlFetcher: Send + Sync {
    /// Fetch `url`. Implementations decide what to do with `data:` URIs
    /// — the renderer's `font_face.rs` strips them before calling, so
    /// fetchers can assume non-data input.
    fn fetch(&self, url: &str, base_dir: Option<&Path>) -> Result<FetchedResource, String>;
}

/// Trait seam for a future pluggable cache (mirrors `WeasyPrint` v68.1's
/// `cache=` parameter). Adding the seam now means a follow-up workstream
/// can plug in a disk- or memory-backed cache without re-threading every
/// call site. There is intentionally no default implementation.
#[allow(dead_code)]
pub trait Cache: Send + Sync {
    /// Look up a cached resource by URL. `None` = cache miss.
    fn get(&self, url: &str) -> Option<FetchedResource>;
    /// Insert a freshly-fetched resource. Implementations may evict.
    fn put(&self, url: &str, resource: &FetchedResource);
}

/// File-only fetcher. The default. Resolves `file://` and bare relative
/// paths against `base_dir`; HTTP/HTTPS schemes always error with the
/// canonical "http-fetch feature not enabled" message regardless of
/// build configuration, so callers wiring this in see consistent
/// behavior whether or not the feature is compiled in.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultFetcher;

impl UrlFetcher for DefaultFetcher {
    fn fetch(&self, url: &str, base_dir: Option<&Path>) -> Result<FetchedResource, String> {
        match parse_scheme(url) {
            Scheme::Http | Scheme::Https => Err("http-fetch feature not enabled".to_string()),
            Scheme::Data => {
                Err("data: URIs must be handled by the resolver, not the fetcher".to_string())
            }
            Scheme::File | Scheme::Other => read_local(url, base_dir),
        }
    }
}

/// Resolve `url` (a `file://` URL or a bare relative/absolute path) into
/// a local path, honoring `base_dir` for relatives, then read the bytes.
fn read_local(url: &str, base_dir: Option<&Path>) -> Result<FetchedResource, String> {
    let path_str = if let Some(rest) = url.strip_prefix("file://") {
        rest
    } else if let Some(rest) = url.strip_prefix("FILE://") {
        rest
    } else {
        url
    };
    let path = Path::new(path_str);
    let resolved: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(base) = base_dir {
        base.join(path)
    } else {
        path.to_path_buf()
    };
    std::fs::read(&resolved)
        .map(|bytes| FetchedResource {
            bytes,
            mime: None,
            redirected_url: None,
        })
        .map_err(|e| format!("failed to read {}: {e}", resolved.display()))
}

/// Placeholder `HttpFetcher` for the `http-fetch` feature. The actual
/// implementation lands in a later rung; this stub exists so the
/// `net::fetch` dispatch compiles when the feature is enabled and so
/// downstream test scaffolding can reference the type.
#[cfg(feature = "http-fetch")]
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpFetcher;

#[cfg(feature = "http-fetch")]
impl UrlFetcher for HttpFetcher {
    fn fetch(&self, _url: &str, _base_dir: Option<&Path>) -> Result<FetchedResource, String> {
        Err("HttpFetcher not yet implemented".to_string())
    }
}

/// Module-level `net::fetch` stub. With the `http-fetch` feature
/// disabled this errors out for any non-`file://` URL — the canonical
/// gate test calls this. With the feature enabled it dispatches to
/// `HttpFetcher` for HTTP(S) and to `DefaultFetcher` for everything
/// else.
pub mod net {
    use super::{DefaultFetcher, FetchedResource, UrlFetcher};
    use std::path::Path;

    #[allow(dead_code)] // Wired into call sites by later rungs.
    pub fn fetch(url: &str, base_dir: Option<&Path>) -> Result<FetchedResource, String> {
        #[cfg(feature = "http-fetch")]
        {
            use super::{Scheme, parse_scheme};
            if matches!(parse_scheme(url), Scheme::Http | Scheme::Https) {
                return super::HttpFetcher.fetch(url, base_dir);
            }
        }
        DefaultFetcher.fetch(url, base_dir)
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

    #[cfg(not(feature = "http-fetch"))]
    #[test]
    fn fetch_without_feature_errors() {
        let err = net::fetch("http://example.com/x", None).expect_err("must fail");
        assert!(
            err.contains("http-fetch feature not enabled"),
            "expected feature-gate error, got {err:?}"
        );
        let err = net::fetch("https://example.com/x", None).expect_err("must fail");
        assert!(
            err.contains("http-fetch feature not enabled"),
            "expected feature-gate error, got {err:?}"
        );
    }

    #[test]
    fn default_fetcher_reads_local_file() {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!("pdfun-stub-{}-{}.txt", std::process::id(), nanos));
        std::fs::write(&path, b"hi").unwrap();
        let res = DefaultFetcher.fetch(path.to_str().unwrap(), None).unwrap();
        assert_eq!(res.bytes, b"hi");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn default_fetcher_rejects_data_uri() {
        let err = DefaultFetcher
            .fetch("data:text/plain,hi", None)
            .expect_err("rejects");
        assert!(err.contains("data:"), "got {err:?}");
    }
}
