use html5ever::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::RcDom;

/// Parse an HTML string into an `RcDom` tree.
pub fn parse_html(html: &str) -> RcDom {
    parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever parsing should not fail")
}
