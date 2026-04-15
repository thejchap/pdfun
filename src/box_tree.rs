// Stage 0 of the box-tree refactor lands the types with no production
// caller — they are exercised only by unit tests in this module and by
// `LayoutInner::push_box_tree` in layout.rs. The dead-code lint fires
// without this allow. Stage 1 switches html_render over and this allow
// gets deleted.
#![allow(dead_code)]

//! CSS box tree — Stage 0 scaffolding.
//!
//! This module introduces a typed tree of boxes that will eventually replace
//! the flat `Vec<Block>` model in `layout.rs`. Stage 0 only adds the types
//! and a `flatten_tree` helper that lowers the tree back into today's flat
//! `Block` list — no production code path uses the tree yet, so behavior is
//! unchanged. The point of this stage is to land the types, wire them into
//! the module graph, and pin their semantics with unit tests so Stage 1 can
//! cut `html_render` and `finish()` over without also debating shapes.
//!
//! See `/home/justinchapman/.claude/plans/staged-rolling-leaf.md` for the
//! full refactor plan.

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

/// Lower a box tree to today's flat `Vec<Block>` list.
///
/// Stage 0 uses this exclusively for unit testing: the scaffolded tree can
/// round-trip through the helper and the flattening preserves every leaf's
/// content. Container-level styling (a `<div>` with its own margin/border)
/// is intentionally dropped during flattening — the flat model has no place
/// to put it. Stage 1 will delete this helper once `finish()` consumes the
/// tree directly.
pub fn flatten_tree(tree: Vec<Node>) -> Vec<Block> {
    let mut out = Vec::new();
    for node in tree {
        flatten_node(node, &mut out);
    }
    out
}

fn flatten_node(node: Node, out: &mut Vec<Block>) {
    match node {
        Node::Block(b) => {
            // Special case: a block containing exactly one anonymous child
            // is today's `Paragraph`. We reconstruct it so the flat output
            // is byte-identical to what `html_render` would push directly.
            if b.children.len() == 1
                && matches!(b.children[0], Node::Anonymous(_))
                && !b.is_hr
            {
                let mut children = b.children;
                let Node::Anonymous(anon) = children.remove(0) else {
                    unreachable!("matched above");
                };
                out.push(Block::Paragraph(Paragraph {
                    runs: anon.runs,
                    line_height: anon.line_height,
                    spacing_after: anon.spacing_after,
                    style: b.style,
                    marker: b.marker,
                    is_hr: false,
                    preserve_whitespace: anon.preserve_whitespace,
                }));
                return;
            }

            // Horizontal-rule leaf: the flat model represents it as a
            // Paragraph with `is_hr = true` and empty runs.
            if b.is_hr {
                out.push(Block::Paragraph(Paragraph {
                    runs: vec![],
                    line_height: None,
                    spacing_after: 12.0,
                    style: b.style,
                    marker: None,
                    is_hr: true,
                    preserve_whitespace: false,
                }));
                return;
            }

            // General case: recurse into children. Container-level styling
            // (the block's own margin/border) is dropped — the flat model
            // cannot represent it. Stage 1 removes this limitation.
            for child in b.children {
                flatten_node(child, out);
            }
        }
        Node::Anonymous(anon) => {
            // A free-standing anonymous box flattens to a paragraph with
            // default style.
            let mut style = BlockStyle::default();
            style.text_align = anon.text_align;
            style.letter_spacing = anon.letter_spacing;
            style.word_spacing = anon.word_spacing;
            out.push(Block::Paragraph(Paragraph {
                runs: anon.runs,
                line_height: anon.line_height,
                spacing_after: anon.spacing_after,
                style,
                marker: None,
                is_hr: false,
                preserve_whitespace: anon.preserve_whitespace,
            }));
        }
        Node::Table(t) => out.push(Block::Table(t.table)),
        Node::Image(i) => out.push(Block::Image(i.image)),
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
    fn paragraph_leaf_round_trips_through_flatten() {
        let tree = vec![Node::paragraph_leaf(sample_paragraph("hello"))];
        let flat = flatten_tree(tree);
        assert_eq!(flat.len(), 1);
        let Block::Paragraph(p) = &flat[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(p.runs.len(), 1);
        assert_eq!(p.runs[0].text, "hello");
        assert!(!p.is_hr);
    }

    #[test]
    fn nested_container_flattens_children_in_order() {
        // BlockBox with two paragraph children — the outer block has no
        // direct anonymous child, so it should recurse and emit both paras
        // in document order.
        let outer = BlockBox {
            style: BlockStyle::default(),
            children: vec![
                Node::paragraph_leaf(sample_paragraph("first")),
                Node::paragraph_leaf(sample_paragraph("second")),
            ],
            tag: Some("div"),
            marker: None,
            spacing_after: 0.0,
            page_break_before: None,
            page_break_after: None,
            is_hr: false,
        };
        let flat = flatten_tree(vec![Node::Block(outer)]);
        assert_eq!(flat.len(), 2);
        let Block::Paragraph(p0) = &flat[0] else {
            panic!();
        };
        let Block::Paragraph(p1) = &flat[1] else {
            panic!();
        };
        assert_eq!(p0.runs[0].text, "first");
        assert_eq!(p1.runs[0].text, "second");
    }

    #[test]
    fn hr_block_flattens_to_hr_paragraph() {
        let hr = BlockBox {
            style: BlockStyle::default(),
            children: vec![],
            tag: Some("hr"),
            marker: None,
            spacing_after: 0.0,
            page_break_before: None,
            page_break_after: None,
            is_hr: true,
        };
        let flat = flatten_tree(vec![Node::Block(hr)]);
        assert_eq!(flat.len(), 1);
        let Block::Paragraph(p) = &flat[0] else {
            panic!();
        };
        assert!(p.is_hr);
        assert!(p.runs.is_empty());
    }

    #[test]
    fn freestanding_anonymous_flattens_to_paragraph() {
        let anon = AnonymousBox {
            runs: vec![sample_run("loose text")],
            line_height: None,
            preserve_whitespace: false,
            text_align: TextAlign::Center,
            letter_spacing: 1.5,
            word_spacing: 2.0,
            spacing_after: 6.0,
        };
        let flat = flatten_tree(vec![Node::Anonymous(anon)]);
        assert_eq!(flat.len(), 1);
        let Block::Paragraph(p) = &flat[0] else {
            panic!();
        };
        assert_eq!(p.runs[0].text, "loose text");
        assert!(matches!(p.style.text_align, TextAlign::Center));
        assert!((p.style.letter_spacing - 1.5).abs() < 1e-6);
        assert!((p.style.word_spacing - 2.0).abs() < 1e-6);
        assert!((p.spacing_after - 6.0).abs() < 1e-6);
    }

    #[test]
    fn mixed_children_preserve_document_order() {
        // div
        //   p "intro"
        //   nested div
        //     p "deep"
        //   p "outro"
        let nested = BlockBox {
            style: BlockStyle::default(),
            children: vec![Node::paragraph_leaf(sample_paragraph("deep"))],
            tag: Some("div"),
            marker: None,
            spacing_after: 0.0,
            page_break_before: None,
            page_break_after: None,
            is_hr: false,
        };
        let outer = BlockBox {
            style: BlockStyle::default(),
            children: vec![
                Node::paragraph_leaf(sample_paragraph("intro")),
                Node::Block(nested),
                Node::paragraph_leaf(sample_paragraph("outro")),
            ],
            tag: Some("div"),
            marker: None,
            spacing_after: 0.0,
            page_break_before: None,
            page_break_after: None,
            is_hr: false,
        };
        let flat = flatten_tree(vec![Node::Block(outer)]);
        assert_eq!(flat.len(), 3);
        let texts: Vec<&str> = flat
            .iter()
            .map(|b| match b {
                Block::Paragraph(p) => p.runs[0].text.as_str(),
                _ => panic!("expected paragraph"),
            })
            .collect();
        assert_eq!(texts, vec!["intro", "deep", "outro"]);
    }
}
