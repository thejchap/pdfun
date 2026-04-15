//! CSS box tree — Stage 1 substrate for `LayoutInner::finish()`.
//!
//! `html_render` still emits a flat `Vec<Block>` with `ContainerStart`/
//! `ContainerEnd` sentinels (legacy streaming API). `LayoutInner::finish()`
//! calls `unflatten_blocks` to reconstruct a proper tree of `Node`s and
//! then walks it recursively. The tree representation is what Stage C's
//! float and inline-block work will read from.

use crate::css;
use crate::layout::{
    Block, BlockStyle, ImageBlock, Paragraph, Table, TextAlign, TextRun,
};

/// A node in the CSS box tree. Containers own their children as `Vec<Node>`
/// so layout can recurse naturally.
pub enum Node {
    /// A block-level container (`<div>`, `<p>`, `<blockquote>`, `<h1>`,
    /// `<li>`, …). May own further block or anonymous children.
    Block(BlockBox),
    /// A run of contiguous inline content inside a block container. These
    /// are created at tree-construction time in `html_render` whenever a
    /// block container has mixed text + block children (CSS 2.1 § 9.2.1.1).
    Anonymous(AnonymousBox),
    /// Self-contained table leaf — cell contents are still flat
    /// `Vec<TextRun>` per the Stage 1 scope cut.
    Table(TableLeaf),
    /// Self-contained image leaf.
    Image(ImageLeaf),
}

/// A block container with its own style and an ordered list of children.
pub struct BlockBox {
    pub style: BlockStyle,
    pub children: Vec<Node>,
    pub tag: Option<&'static str>,
    /// List marker (disc, decimal, etc.) painted before the first line of
    /// the first anonymous child. Only set for `<li>`.
    pub marker: Option<TextRun>,
    /// UA-default "gap below this block" — e.g. 12pt after a `<p>`. This is
    /// *not* a CSS margin and does not participate in collapsing.
    pub spacing_after: f32,
    pub page_break_before: Option<css::PageBreak>,
    pub page_break_after: Option<css::PageBreak>,
    pub is_hr: bool,
}

/// A purely-inline flow of text runs, used for mixed-content containers.
///
/// Anonymous boxes exist so that a block container can carry its own
/// margin/padding/border while still painting inline text. Today's
/// `Paragraph` maps onto `BlockBox { children: vec![Node::Anonymous(_)] }`.
pub struct AnonymousBox {
    pub runs: Vec<TextRun>,
    pub line_height: Option<f32>,
    pub preserve_whitespace: bool,
    pub text_align: TextAlign,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    /// UA-default gap below (same meaning as `BlockBox::spacing_after`).
    pub spacing_after: f32,
}

/// Wraps an existing `layout::Table` — tables are leaves in Stage 1.
pub struct TableLeaf {
    pub table: Table,
}

/// Wraps an existing `layout::ImageBlock` — images are leaves.
pub struct ImageLeaf {
    pub image: ImageBlock,
}

impl Node {
    /// Build a leaf node wrapping today's `Paragraph` shape. Used by
    /// `flatten_tree` round-trip tests and by Stage 1 construction sites.
    pub fn paragraph_leaf(para: Paragraph) -> Node {
        let style = para.style.clone();
        let anon = AnonymousBox {
            runs: para.runs,
            line_height: para.line_height,
            preserve_whitespace: para.preserve_whitespace,
            text_align: style.text_align,
            letter_spacing: style.letter_spacing,
            word_spacing: style.word_spacing,
            spacing_after: para.spacing_after,
        };
        Node::Block(BlockBox {
            style: para.style,
            children: vec![Node::Anonymous(anon)],
            tag: None,
            marker: para.marker,
            spacing_after: 0.0,
            page_break_before: None,
            page_break_after: None,
            is_hr: para.is_hr,
        })
    }
}

/// Reconstruct a box tree from a flat `Vec<Block>` stream.
///
/// `html_render` still uses the legacy streaming API (`push_paragraph`,
/// `push_container_start`, etc.) which produces a flat list with
/// `ContainerStart(style)` / `ContainerEnd(style)` sentinels around each
/// block container. This helper parses those sentinels into nested
/// `BlockBox`es so `LayoutInner::finish()` can walk the tree recursively.
///
/// Input shape:
/// - `Block::Paragraph` → `Node::Block(BlockBox { children: [Anonymous] })`
///   (or `is_hr=true` for horizontal rules).
/// - `Block::Table` → `Node::Table(TableLeaf)`.
/// - `Block::Image` → `Node::Image(ImageLeaf)`.
/// - `Block::ContainerStart(style)` opens a new frame; subsequent blocks
///   become its children until the matching `Block::ContainerEnd`, which
///   pops the frame and wraps it in a `BlockBox` with that style.
pub fn unflatten_blocks(blocks: Vec<Block>) -> Vec<Node> {
    // Each open frame is (container_style, children_so_far). The root
    // frame has no style — it represents the top-level tree.
    let mut frames: Vec<(Option<BlockStyle>, Vec<Node>)> = vec![(None, Vec::new())];

    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                let node = if p.is_hr {
                    Node::Block(BlockBox {
                        style: p.style,
                        children: Vec::new(),
                        tag: Some("hr"),
                        marker: None,
                        spacing_after: p.spacing_after,
                        page_break_before: None,
                        page_break_after: None,
                        is_hr: true,
                    })
                } else {
                    Node::paragraph_leaf(p)
                };
                frames.last_mut().unwrap().1.push(node);
            }
            Block::Table(t) => {
                frames
                    .last_mut()
                    .unwrap()
                    .1
                    .push(Node::Table(TableLeaf { table: t }));
            }
            Block::Image(i) => {
                frames
                    .last_mut()
                    .unwrap()
                    .1
                    .push(Node::Image(ImageLeaf { image: i }));
            }
            Block::ContainerStart(style) => {
                frames.push((Some(style), Vec::new()));
            }
            Block::ContainerEnd(_end_style) => {
                // The start and end sentinels currently carry the same
                // style — we use the one recorded at start time for
                // consistency.
                let (start_style, children) = frames
                    .pop()
                    .expect("ContainerEnd without matching ContainerStart");
                let style =
                    start_style.expect("non-root frame must have recorded style");
                let page_break_before = style.page_break_before;
                let page_break_after = style.page_break_after;
                let bb = BlockBox {
                    style,
                    children,
                    tag: None,
                    marker: None,
                    spacing_after: 0.0,
                    page_break_before,
                    page_break_after,
                    is_hr: false,
                };
                frames.last_mut().unwrap().1.push(Node::Block(bb));
            }
        }
    }

    // Implicit close for any containers left open by a malformed flat
    // stream. html_render's own nesting guarantees this shouldn't happen,
    // but closing them defensively keeps `finish()` from panicking.
    while frames.len() > 1 {
        let (start_style, children) = frames.pop().unwrap();
        let style = start_style.unwrap_or_default();
        let bb = BlockBox {
            style,
            children,
            tag: None,
            marker: None,
            spacing_after: 0.0,
            page_break_before: None,
            page_break_after: None,
            is_hr: false,
        };
        frames.last_mut().unwrap().1.push(Node::Block(bb));
    }

    frames.pop().unwrap().1
}

/// Returns `Some(&AnonymousBox)` if this `BlockBox` is the "paragraph
/// shape" produced by `paragraph_leaf` — exactly one anonymous child,
/// not an HR. Used by the recursive renderer to dispatch paragraph
/// rendering vs. generic container walking.
pub fn paragraph_shape(bb: &BlockBox) -> Option<&AnonymousBox> {
    if bb.is_hr || bb.children.len() != 1 {
        return None;
    }
    match &bb.children[0] {
        Node::Anonymous(anon) => Some(anon),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_run(text: &str) -> TextRun {
        TextRun {
            text: text.to_string(),
            font_name: "Helvetica".to_string(),
            font_size: 12.0,
            color: None,
            text_decoration: None,
            link_url: None,
        }
    }

    fn sample_paragraph(text: &str) -> Paragraph {
        Paragraph {
            runs: vec![sample_run(text)],
            line_height: None,
            spacing_after: 12.0,
            style: BlockStyle::default(),
            marker: None,
            is_hr: false,
            preserve_whitespace: false,
        }
    }

    #[test]
    fn unflatten_single_paragraph_produces_paragraph_shape_block() {
        let flat = vec![Block::Paragraph(sample_paragraph("hello"))];
        let tree = unflatten_blocks(flat);
        assert_eq!(tree.len(), 1);
        let Node::Block(bb) = &tree[0] else {
            panic!("expected Block");
        };
        let anon = paragraph_shape(bb).expect("should be paragraph shape");
        assert_eq!(anon.runs.len(), 1);
        assert_eq!(anon.runs[0].text, "hello");
    }

    #[test]
    fn unflatten_hr_produces_hr_block() {
        let flat = vec![Block::Paragraph(Paragraph {
            runs: vec![],
            line_height: None,
            spacing_after: 12.0,
            style: BlockStyle::default(),
            marker: None,
            is_hr: true,
            preserve_whitespace: false,
        })];
        let tree = unflatten_blocks(flat);
        assert_eq!(tree.len(), 1);
        let Node::Block(bb) = &tree[0] else {
            panic!();
        };
        assert!(bb.is_hr);
        assert!(bb.children.is_empty());
        assert!(paragraph_shape(bb).is_none());
    }

    #[test]
    fn unflatten_container_sentinels_produces_nested_block() {
        let mut outer_style = BlockStyle::default();
        outer_style.margin_top = 10.0;
        outer_style.margin_bottom = 10.0;
        let flat = vec![
            Block::ContainerStart(outer_style.clone()),
            Block::Paragraph(sample_paragraph("inside")),
            Block::ContainerEnd(outer_style.clone()),
        ];
        let tree = unflatten_blocks(flat);
        assert_eq!(tree.len(), 1);
        let Node::Block(outer) = &tree[0] else {
            panic!();
        };
        assert!((outer.style.margin_top - 10.0).abs() < 1e-6);
        assert!((outer.style.margin_bottom - 10.0).abs() < 1e-6);
        assert_eq!(outer.children.len(), 1);
        let Node::Block(inner) = &outer.children[0] else {
            panic!();
        };
        let anon = paragraph_shape(inner).expect("inner should be paragraph shape");
        assert_eq!(anon.runs[0].text, "inside");
    }

    #[test]
    fn unflatten_mixed_children_preserve_document_order() {
        let outer_style = BlockStyle::default();
        let inner_style = BlockStyle::default();
        let flat = vec![
            Block::ContainerStart(outer_style.clone()),
            Block::Paragraph(sample_paragraph("intro")),
            Block::ContainerStart(inner_style.clone()),
            Block::Paragraph(sample_paragraph("deep")),
            Block::ContainerEnd(inner_style),
            Block::Paragraph(sample_paragraph("outro")),
            Block::ContainerEnd(outer_style),
        ];
        let tree = unflatten_blocks(flat);
        assert_eq!(tree.len(), 1);
        let Node::Block(outer) = &tree[0] else {
            panic!();
        };
        assert_eq!(outer.children.len(), 3);
        let texts: Vec<&str> = outer
            .children
            .iter()
            .map(|n| match n {
                Node::Block(bb) => {
                    if let Some(anon) = paragraph_shape(bb) {
                        anon.runs[0].text.as_str()
                    } else {
                        bb.children
                            .iter()
                            .find_map(|c| match c {
                                Node::Block(inner) => {
                                    paragraph_shape(inner).map(|a| a.runs[0].text.as_str())
                                }
                                _ => None,
                            })
                            .unwrap_or("?")
                    }
                }
                _ => "?",
            })
            .collect();
        assert_eq!(texts, vec!["intro", "deep", "outro"]);
    }

    #[test]
    fn unflatten_container_page_breaks_propagate_to_block_box() {
        let mut style = BlockStyle::default();
        style.page_break_before = Some(css::PageBreak::Always);
        style.page_break_after = Some(css::PageBreak::Always);
        let flat = vec![
            Block::ContainerStart(style.clone()),
            Block::ContainerEnd(style),
        ];
        let tree = unflatten_blocks(flat);
        let Node::Block(bb) = &tree[0] else {
            panic!();
        };
        assert!(matches!(bb.page_break_before, Some(css::PageBreak::Always)));
        assert!(matches!(bb.page_break_after, Some(css::PageBreak::Always)));
    }
}
