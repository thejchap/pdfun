#![allow(clippy::cast_possible_truncation)] // f64->f32 at PyO3 boundary is intentional
#![allow(clippy::cast_precision_loss)] // u32/usize->f32 precision loss is acceptable
#![allow(clippy::cast_possible_wrap)] // usize->i32 for page count is fine
#![allow(clippy::unnecessary_wraps)] // PyO3 #[pymethods] requires PyResult signatures

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pdf_writer::types::{ActionType, AnnotationType, CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

mod css;
mod dom;
mod font_metrics;
mod html_render;
mod layout;

// ── Built-in PDF fonts ─────────────────────────────────────────

pub(crate) const BUILTIN_FONTS: &[&str] = &[
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

// ── Internal types ─────────────────────────────────────────────

pub(crate) enum PdfOp {
    BeginText,
    EndText,
    SetFont { name: String, size: f32 },
    SetTextPosition { x: f32, y: f32 },
    ShowText(String),
    /// glyph ids are resolved lazily in `to_bytes()`; stores (char,) pairs
    ShowGlyphs(Vec<char>),
    // Graphics primitives
    SetFillColor { r: f32, g: f32, b: f32 },
    SetStrokeColor { r: f32, g: f32, b: f32 },
    SetLineWidth(f32),
    Rectangle { x: f32, y: f32, width: f32, height: f32 },
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    Stroke,
    Fill,
    FillAndStroke,
    SaveState,
    RestoreState,
    SetDashPattern { array: Vec<f32>, phase: f32 },
}

pub(crate) struct LinkAnnotation {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) url: String,
}

pub(crate) struct PageContent {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) operations: Vec<PdfOp>,
    pub(crate) fonts_used: Vec<String>,
    pub(crate) current_font: Option<String>,
    pub(crate) current_font_size: Option<f32>,
    pub(crate) links: Vec<LinkAnnotation>,
}

impl PageContent {
    fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            operations: Vec::new(),
            fonts_used: Vec::new(),
            current_font: None,
            current_font_size: None,
            links: Vec::new(),
        }
    }
}

/// Escape a string for PDF literal string encoding.
/// Parentheses and backslashes must be escaped.
fn pdf_escape(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'(' => out.extend_from_slice(b"\\("),
            b')' => out.extend_from_slice(b"\\)"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            _ => out.push(b),
        }
    }
    out
}

// ── Registered (embedded) fonts ────────────────────────────────

struct RegisteredFont {
    data: Vec<u8>,
    family: String,
    name: String, // e.g. "Custom-0"
}

// ── PdfDocument ────────────────────────────────────────────────

#[pyclass]
pub(crate) struct PdfDocument {
    pub(crate) pages: Vec<Arc<Mutex<PageContent>>>,
    registered_fonts: Vec<RegisteredFont>,
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) subject: Option<String>,
    pub(crate) keywords: Option<String>,
    pub(crate) creator: Option<String>,
}

#[pymethods]
impl PdfDocument {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(PdfDocument {
            pages: Vec::new(),
            registered_fonts: Vec::new(),
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: Some("pdfun".to_string()),
        })
    }

    fn set_title(&mut self, title: &str) -> PyResult<()> {
        self.title = Some(title.to_string());
        Ok(())
    }

    fn set_author(&mut self, author: &str) -> PyResult<()> {
        self.author = Some(author.to_string());
        Ok(())
    }

    fn set_subject(&mut self, subject: &str) -> PyResult<()> {
        self.subject = Some(subject.to_string());
        Ok(())
    }

    fn set_keywords(&mut self, keywords: &str) -> PyResult<()> {
        self.keywords = Some(keywords.to_string());
        Ok(())
    }

    #[pyo3(signature = (width=612.0, height=792.0))]
    fn add_page(&mut self, width: f64, height: f64) -> PyResult<Page> {
        let content = Arc::new(Mutex::new(PageContent::new(width, height)));
        self.pages.push(Arc::clone(&content));
        Ok(Page { content })
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::items_after_statements)]
    fn to_bytes(&self) -> PyResult<Vec<u8>> {
        let mut pdf = Pdf::new();
        let mut next_id = 1;
        let mut alloc = || {
            let id = Ref::new(next_id);
            next_id += 1;
            id
        };

        let catalog_id = alloc();
        let page_tree_id = alloc();

        // pre-allocate refs for each page (page obj + content stream)
        let page_refs: Vec<(Ref, Ref)> = self.pages.iter().map(|_| (alloc(), alloc())).collect();

        // collect all unique font names across all pages
        let mut all_fonts: Vec<String> = Vec::new();
        for page_arc in &self.pages {
            let page = page_arc.lock().unwrap();
            for font_name in &page.fonts_used {
                if !all_fonts.contains(font_name) {
                    all_fonts.push(font_name.clone());
                }
            }
        }

        // allocate font refs: builtin fonts get 1 ref, custom fonts get 5
        // (type0, cid_font, font_descriptor, font_file, tounicode)
        struct FontRefs {
            name: String,
            type0_ref: Ref,
            cid_font_ref: Option<Ref>,
            font_descriptor_ref: Option<Ref>,
            font_file_ref: Option<Ref>,
            tounicode_ref: Option<Ref>,
        }

        let font_refs: Vec<FontRefs> = all_fonts
            .iter()
            .map(|name| {
                let is_custom = name.starts_with("Custom-");
                let type0_ref = alloc();
                FontRefs {
                    name: name.clone(),
                    type0_ref,
                    cid_font_ref: if is_custom { Some(alloc()) } else { None },
                    font_descriptor_ref: if is_custom { Some(alloc()) } else { None },
                    font_file_ref: if is_custom { Some(alloc()) } else { None },
                    tounicode_ref: if is_custom { Some(alloc()) } else { None },
                }
            })
            .collect();

        // for custom fonts, parse face data and collect glyph info from all pages
        // key: custom font name -> (RegisteredFont index, collected chars)
        let mut custom_font_chars: BTreeMap<String, Vec<char>> = BTreeMap::new();
        for page_arc in &self.pages {
            let page = page_arc.lock().unwrap();
            let mut current_font_name: Option<String> = None;
            for op in &page.operations {
                match op {
                    PdfOp::SetFont { name, .. } => {
                        current_font_name = Some(name.clone());
                    }
                    PdfOp::ShowGlyphs(chars) => {
                        if let Some(ref fname) = current_font_name {
                            custom_font_chars
                                .entry(fname.clone())
                                .or_default()
                                .extend(chars);
                        }
                    }
                    _ => {}
                }
            }
        }

        // build glyph mappings for each custom font
        // font_name -> (face, remapper, subset_data, gid_map: char -> new_gid, widths: new_gid -> width)
        struct CustomFontData {
            subset_data: Vec<u8>,
            // char -> new glyph id
            char_to_new_gid: BTreeMap<char, u16>,
            // new_gid -> width (1000-unit scale)
            gid_widths: BTreeMap<u16, f32>,
            // font metrics (1000-unit scale)
            ascent: f32,
            descent: f32,
            cap_height: f32,
            bbox: Rect,
            family: String,
        }

        let mut custom_data: BTreeMap<String, CustomFontData> = BTreeMap::new();

        for rf in &self.registered_fonts {
            let Some(chars) = custom_font_chars.get(&rf.name) else {
                continue;
            };

            let face = ttf_parser::Face::parse(&rf.data, 0).map_err(|e| {
                PyValueError::new_err(format!("failed to parse font for embedding: {e}"))
            })?;

            let units_per_em = f32::from(face.units_per_em());

            // build glyph remapper
            let mut remapper = subsetter::GlyphRemapper::new();
            let mut char_to_old_gid: BTreeMap<char, u16> = BTreeMap::new();

            for &ch in chars {
                if let Some(gid) = face.glyph_index(ch) {
                    let old_gid = gid.0;
                    remapper.remap(old_gid);
                    char_to_old_gid.insert(ch, old_gid);
                }
            }

            // subset the font (fall back to full data if subsetting fails)
            let subset_data = subsetter::subset(&rf.data, 0, &remapper)
                .unwrap_or_else(|_| rf.data.clone());

            // build char -> new_gid mapping and gid -> width
            let mut char_to_new_gid: BTreeMap<char, u16> = BTreeMap::new();
            let mut gid_widths: BTreeMap<u16, f32> = BTreeMap::new();

            for (&ch, &old_gid) in &char_to_old_gid {
                if let Some(new_gid) = remapper.get(old_gid) {
                    char_to_new_gid.insert(ch, new_gid);
                    let width = f32::from(
                        face.glyph_hor_advance(ttf_parser::GlyphId(old_gid))
                            .unwrap_or(0),
                    );
                    // convert to 1000-unit scale
                    let scaled_width = width * 1000.0 / units_per_em;
                    gid_widths.insert(new_gid, scaled_width);
                }
            }

            let ascent = f32::from(face.ascender()) * 1000.0 / units_per_em;
            let descent = f32::from(face.descender()) * 1000.0 / units_per_em;
            let cap_height = f32::from(face.capital_height().unwrap_or(face.ascender())) * 1000.0
                / units_per_em;
            let global_bbox = face.global_bounding_box();
            let bbox = Rect::new(
                f32::from(global_bbox.x_min) * 1000.0 / units_per_em,
                f32::from(global_bbox.y_min) * 1000.0 / units_per_em,
                f32::from(global_bbox.x_max) * 1000.0 / units_per_em,
                f32::from(global_bbox.y_max) * 1000.0 / units_per_em,
            );

            custom_data.insert(
                rf.name.clone(),
                CustomFontData {
                    subset_data,
                    char_to_new_gid,
                    gid_widths,
                    ascent,
                    descent,
                    cap_height,
                    bbox,
                    family: rf.family.clone(),
                },
            );
        }

        // write catalog
        pdf.catalog(catalog_id).pages(page_tree_id);

        // write document metadata
        let has_info = self.title.is_some()
            || self.author.is_some()
            || self.subject.is_some()
            || self.keywords.is_some()
            || self.creator.is_some();
        if has_info {
            let info_id = alloc();
            let mut info = pdf.document_info(info_id);
            if let Some(ref title) = self.title {
                info.title(pdf_writer::TextStr(title));
            }
            if let Some(ref author) = self.author {
                info.author(pdf_writer::TextStr(author));
            }
            if let Some(ref subject) = self.subject {
                info.subject(pdf_writer::TextStr(subject));
            }
            if let Some(ref keywords) = self.keywords {
                info.keywords(pdf_writer::TextStr(keywords));
            }
            if let Some(ref creator) = self.creator {
                info.creator(pdf_writer::TextStr(creator));
            }
        }

        // write page tree
        let page_ids: Vec<Ref> = page_refs.iter().map(|(pid, _)| *pid).collect();
        pdf.pages(page_tree_id)
            .kids(page_ids)
            .count(self.pages.len() as i32);

        // write each page
        for (i, page_arc) in self.pages.iter().enumerate() {
            let page = page_arc.lock().unwrap();
            let (page_id, content_id) = page_refs[i];

            // build content stream
            let mut content = Content::new();
            let mut current_font_name: Option<String> = None;

            for op in &page.operations {
                match op {
                    PdfOp::BeginText => {
                        content.begin_text();
                    }
                    PdfOp::EndText => {
                        content.end_text();
                    }
                    PdfOp::SetFont { name, size } => {
                        current_font_name = Some(name.clone());
                        let idx = font_refs
                            .iter()
                            .position(|fr| fr.name == *name)
                            .unwrap_or(0);
                        let resource_name = format!("F{}", idx + 1);
                        content.set_font(Name(resource_name.as_bytes()), *size);
                    }
                    PdfOp::SetTextPosition { x, y } => {
                        content.next_line(*x, *y);
                    }
                    PdfOp::ShowText(text) => {
                        let escaped = pdf_escape(text);
                        content.show(Str(&escaped));
                    }
                    PdfOp::ShowGlyphs(chars) => {
                        // resolve glyph ids using the custom font data
                        if let Some(ref fname) = current_font_name
                            && let Some(cfd) = custom_data.get(fname)
                        {
                            let mut bytes = Vec::with_capacity(chars.len() * 2);
                            for &ch in chars {
                                let gid = cfd.char_to_new_gid.get(&ch).copied().unwrap_or(0);
                                bytes.push((gid >> 8) as u8);
                                bytes.push((gid & 0xff) as u8);
                            }
                            content.show(Str(&bytes));
                        }
                    }
                    PdfOp::SetFillColor { r, g, b } => {
                        content.set_fill_rgb(*r, *g, *b);
                    }
                    PdfOp::SetStrokeColor { r, g, b } => {
                        content.set_stroke_rgb(*r, *g, *b);
                    }
                    PdfOp::SetLineWidth(w) => {
                        content.set_line_width(*w);
                    }
                    PdfOp::Rectangle { x, y, width, height } => {
                        content.rect(*x, *y, *width, *height);
                    }
                    PdfOp::MoveTo { x, y } => {
                        content.move_to(*x, *y);
                    }
                    PdfOp::LineTo { x, y } => {
                        content.line_to(*x, *y);
                    }
                    PdfOp::Stroke => {
                        content.stroke();
                    }
                    PdfOp::Fill => {
                        content.fill_nonzero();
                    }
                    PdfOp::FillAndStroke => {
                        content.fill_nonzero_and_stroke();
                    }
                    PdfOp::SaveState => {
                        content.save_state();
                    }
                    PdfOp::RestoreState => {
                        content.restore_state();
                    }
                    PdfOp::SetDashPattern { array, phase } => {
                        content.set_dash_pattern(array.iter().copied(), *phase);
                    }
                }
            }

            let content_bytes = content.finish();

            // pre-allocate refs for link annotations on this page
            let annot_refs: Vec<Ref> = page.links.iter().map(|_| alloc()).collect();

            // write page object
            {
                let mut page_writer = pdf.page(page_id);
                page_writer
                    .parent(page_tree_id)
                    .media_box(Rect::new(0.0, 0.0, page.width as f32, page.height as f32))
                    .contents(content_id);

                if !annot_refs.is_empty() {
                    page_writer.annotations(annot_refs.iter().copied());
                }

                let mut resources = page_writer.resources();
                let mut fonts_dict = resources.fonts();
                for (idx, fr) in font_refs.iter().enumerate() {
                    let resource_name = format!("F{}", idx + 1);
                    fonts_dict.pair(Name(resource_name.as_bytes()), fr.type0_ref);
                }
            }

            // write link annotation objects
            for (annot_ref, link) in annot_refs.iter().zip(page.links.iter()) {
                let rect = Rect::new(
                    link.x,
                    link.y,
                    link.x + link.width,
                    link.y + link.height,
                );
                let mut annot = pdf.annotation(*annot_ref);
                annot
                    .subtype(AnnotationType::Link)
                    .rect(rect)
                    .border(0.0, 0.0, 0.0, None);
                annot
                    .action()
                    .action_type(ActionType::Uri)
                    .uri(Str(link.url.as_bytes()));
            }

            // write content stream
            pdf.stream(content_id, &content_bytes);
        }

        // write font objects
        for fr in &font_refs {
            if fr.name.starts_with("Custom-") {
                // composite font
                if let Some(cfd) = custom_data.get(&fr.name) {
                    let cid_font_ref = fr.cid_font_ref.unwrap();
                    let font_descriptor_ref = fr.font_descriptor_ref.unwrap();
                    let font_file_ref = fr.font_file_ref.unwrap();
                    let tounicode_ref = fr.tounicode_ref.unwrap();

                    // sanitize family name for PDF (replace spaces with hyphens)
                    let base_font_name = cfd.family.replace(' ', "-");

                    // write type0 font
                    pdf.type0_font(fr.type0_ref)
                        .base_font(Name(base_font_name.as_bytes()))
                        .encoding_predefined(Name(b"Identity-H"))
                        .descendant_font(cid_font_ref)
                        .to_unicode(tounicode_ref);

                    // write cid font
                    {
                        let mut cid = pdf.cid_font(cid_font_ref);
                        cid.subtype(CidFontType::Type2)
                            .base_font(Name(base_font_name.as_bytes()))
                            .system_info(SystemInfo {
                                registry: Str(b"Adobe"),
                                ordering: Str(b"Identity"),
                                supplement: 0,
                            })
                            .font_descriptor(font_descriptor_ref)
                            .cid_to_gid_map_predefined(Name(b"Identity"));

                        // write /W widths array
                        let mut widths = cid.widths();
                        for (&gid, &w) in &cfd.gid_widths {
                            widths.consecutive(gid, [w]);
                        }
                    }

                    // write font descriptor
                    pdf.font_descriptor(font_descriptor_ref)
                        .name(Name(base_font_name.as_bytes()))
                        .flags(FontFlags::NON_SYMBOLIC)
                        .bbox(cfd.bbox)
                        .italic_angle(0.0)
                        .ascent(cfd.ascent)
                        .descent(cfd.descent)
                        .cap_height(cfd.cap_height)
                        .stem_v(80.0)
                        .font_file2(font_file_ref);

                    // write font file stream (subset data)
                    pdf.stream(font_file_ref, &cfd.subset_data);

                    // write tounicode cmap
                    let sys_info = SystemInfo {
                        registry: Str(b"Adobe"),
                        ordering: Str(b"Identity"),
                        supplement: 0,
                    };
                    let mut cmap = UnicodeCmap::new(Name(b"Custom"), sys_info);
                    for (&ch, &new_gid) in &cfd.char_to_new_gid {
                        cmap.pair(new_gid, ch);
                    }
                    let cmap_data = cmap.finish();
                    pdf.stream(tounicode_ref, cmap_data.as_slice());
                }
            } else {
                // builtin font
                pdf.type1_font(fr.type0_ref)
                    .base_font(Name(fr.name.as_bytes()));
            }
        }

        Ok(pdf.finish())
    }

    fn save(&self, path: &str) -> PyResult<()> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)
            .map_err(|e| PyValueError::new_err(format!("failed to write file: {e}")))?;
        Ok(())
    }

    fn register_font(&mut self, font_db: &FontDatabase, font_id: &FontId) -> PyResult<String> {
        let entry = font_db
            .fonts
            .get(font_id.index)
            .ok_or_else(|| PyValueError::new_err("invalid font id"))?;
        let name = format!("Custom-{}", self.registered_fonts.len());
        self.registered_fonts.push(RegisteredFont {
            data: entry.data.clone(),
            family: entry.family.clone(),
            name: name.clone(),
        });
        Ok(name)
    }
}

// ── Page ───────────────────────────────────────────────────────

#[pyclass]
struct Page {
    content: Arc<Mutex<PageContent>>,
}

#[pymethods]
impl Page {
    #[getter]
    fn width(&self) -> PyResult<f64> {
        Ok(self.content.lock().unwrap().width)
    }

    #[getter]
    fn height(&self) -> PyResult<f64> {
        Ok(self.content.lock().unwrap().height)
    }

    fn set_font(&mut self, name: &str, size: f64) -> PyResult<()> {
        let is_builtin = BUILTIN_FONTS.contains(&name);
        let is_custom = name.starts_with("Custom-");
        if !is_builtin && !is_custom {
            return Err(PyValueError::new_err(format!("unknown font: {name}")));
        }
        let mut page = self.content.lock().unwrap();
        // track that this page uses this font
        if !page.fonts_used.contains(&name.to_string()) {
            page.fonts_used.push(name.to_string());
        }
        page.current_font = Some(name.to_string());
        page.current_font_size = Some(size as f32);
        page.operations.push(PdfOp::BeginText);
        page.operations.push(PdfOp::SetFont {
            name: name.to_string(),
            size: size as f32,
        });
        Ok(())
    }

    fn measure_text(&self, text: &str) -> PyResult<f64> {
        let page = self.content.lock().unwrap();
        let font_name = page
            .current_font
            .as_deref()
            .ok_or_else(|| PyValueError::new_err("no font set; call set_font() first"))?;
        let font_size = page.current_font_size.unwrap();
        let width = font_metrics::measure_str(text, font_name, font_size)
            .ok_or_else(|| PyValueError::new_err(format!("no metrics for font: {font_name}")))?;
        Ok(f64::from(width))
    }

    fn draw_text(&mut self, x: f64, y: f64, text: &str) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        let is_custom = page
            .current_font
            .as_deref()
            .is_some_and(|f| f.starts_with("Custom-"));
        page.operations.push(PdfOp::SetTextPosition {
            x: x as f32,
            y: y as f32,
        });
        if is_custom {
            page.operations
                .push(PdfOp::ShowGlyphs(text.chars().collect()));
        } else {
            page.operations.push(PdfOp::ShowText(text.to_string()));
        }
        page.operations.push(PdfOp::EndText);
        Ok(())
    }

    fn set_fill_color(&mut self, r: f64, g: f64, b: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetFillColor {
            r: r as f32,
            g: g as f32,
            b: b as f32,
        });
        Ok(())
    }

    fn set_stroke_color(&mut self, r: f64, g: f64, b: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetStrokeColor {
            r: r as f32,
            g: g as f32,
            b: b as f32,
        });
        Ok(())
    }

    fn set_line_width(&mut self, width: f64) -> PyResult<()> {
        if width < 0.0 {
            return Err(PyValueError::new_err("line width must be non-negative"));
        }
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetLineWidth(width as f32));
        Ok(())
    }

    fn draw_rect(&mut self, x: f64, y: f64, width: f64, height: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        });
        page.operations.push(PdfOp::Fill);
        Ok(())
    }

    fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        });
        page.operations.push(PdfOp::Stroke);
        Ok(())
    }

    fn fill_and_stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        });
        page.operations.push(PdfOp::FillAndStroke);
        Ok(())
    }

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::MoveTo {
            x: x1 as f32,
            y: y1 as f32,
        });
        page.operations.push(PdfOp::LineTo {
            x: x2 as f32,
            y: y2 as f32,
        });
        page.operations.push(PdfOp::Stroke);
        Ok(())
    }

    fn save_state(&mut self) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SaveState);
        Ok(())
    }

    fn restore_state(&mut self) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::RestoreState);
        Ok(())
    }
}

// ── FontId ────────────────────────────────────────────────────

#[pyclass]
#[derive(Clone)]
struct FontId {
    index: usize,
}

#[pymethods]
impl FontId {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(FontId { index: 0 })
    }
}

// ── FontDatabase ──────────────────────────────────────────────

struct FontEntry {
    data: Vec<u8>,
    family: String,
    weight: u16,
    italic: bool,
}

#[pyclass]
struct FontDatabase {
    fonts: Vec<FontEntry>,
}

#[pymethods]
impl FontDatabase {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(FontDatabase { fonts: Vec::new() })
    }

    fn load_system_fonts(&mut self) -> PyResult<()> {
        let dirs = if cfg!(target_os = "macos") {
            vec![
                "/System/Library/Fonts".to_string(),
                "/Library/Fonts".to_string(),
            ]
        } else if cfg!(target_os = "linux") {
            vec![
                "/usr/share/fonts".to_string(),
                "/usr/local/share/fonts".to_string(),
            ]
        } else if cfg!(target_os = "windows") {
            vec!["C:\\Windows\\Fonts".to_string()]
        } else {
            vec![]
        };

        for dir in dirs {
            self.scan_font_dir(&dir);
        }
        Ok(())
    }

    fn load_font_file(&mut self, path: &str) -> PyResult<FontId> {
        let data = std::fs::read(path)
            .map_err(|e| PyValueError::new_err(format!("failed to read font file: {e}")))?;
        self.load_font_data_impl(&data)
    }

    fn load_font_data(&mut self, data: &[u8]) -> PyResult<FontId> {
        self.load_font_data_impl(data)
    }

    #[pyo3(signature = (family, weight=None, italic=None))]
    fn query(
        &self,
        family: &str,
        weight: Option<u16>,
        italic: Option<bool>,
    ) -> PyResult<Option<FontId>> {
        let family_lower = family.to_lowercase();
        let mut best: Option<(usize, i32)> = None;

        for (i, entry) in self.fonts.iter().enumerate() {
            if entry.family.to_lowercase() != family_lower {
                continue;
            }
            let mut score: i32 = 0;
            if let Some(w) = weight {
                score -= (i32::from(entry.weight) - i32::from(w)).abs();
            }
            if let Some(it) = italic
                && entry.italic != it
            {
                score -= 1000;
            }
            if best.is_none() || score > best.unwrap().1 {
                best = Some((i, score));
            }
        }

        Ok(best.map(|(index, _)| FontId { index }))
    }
}

impl FontDatabase {
    fn load_font_data_impl(&mut self, data: &[u8]) -> PyResult<FontId> {
        let face = ttf_parser::Face::parse(data, 0)
            .map_err(|e| PyValueError::new_err(format!("invalid font data: {e}")))?;

        let family = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let weight = face.weight().to_number();
        let italic = face.is_italic();

        let index = self.fonts.len();
        self.fonts.push(FontEntry {
            data: data.to_vec(),
            family,
            weight,
            italic,
        });

        Ok(FontId { index })
    }

    fn scan_font_dir(&mut self, dir: &str) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_font_dir(&path.to_string_lossy());
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc")
                && let Ok(data) = std::fs::read(&path)
            {
                let _ = self.load_font_data_impl(&data);
            }
        }
    }
}

/// Render HTML to a PDF document (called from Python `HtmlDocument`).
#[pyfunction]
#[pyo3(signature = (html, margin_top=72.0, margin_right=72.0, margin_bottom=72.0, margin_left=72.0, page_width=612.0, page_height=792.0))]
#[allow(clippy::too_many_arguments)]
fn html_to_pdf(
    html: &str,
    margin_top: f64,
    margin_right: f64,
    margin_bottom: f64,
    margin_left: f64,
    page_width: f64,
    page_height: f64,
) -> PyResult<PdfDocument> {
    let parsed = dom::parse_html(html);
    let title = html_render::extract_title(&parsed.document);
    let mut doc = PdfDocument {
        pages: Vec::new(),
        registered_fonts: Vec::new(),
        title,
        author: None,
        subject: None,
        keywords: None,
        creator: Some("pdfun".to_string()),
    };
    let mut inner = layout::LayoutInner::new(
        margin_top as f32,
        margin_right as f32,
        margin_bottom as f32,
        margin_left as f32,
        page_width as f32,
        page_height as f32,
    );
    let page_style = html_render::render_dom_to_layout(&parsed.document, &mut inner);

    // Apply @page CSS overrides
    if let Some(w) = page_style.width {
        inner.page_width = w;
    }
    if let Some(h) = page_style.height {
        inner.page_height = h;
    }
    if let Some(m) = page_style.margin_top {
        inner.margin_top = m;
    }
    if let Some(m) = page_style.margin_right {
        inner.margin_right = m;
    }
    if let Some(m) = page_style.margin_bottom {
        inner.margin_bottom = m;
    }
    if let Some(m) = page_style.margin_left {
        inner.margin_left = m;
    }

    inner.finish(&mut doc).map_err(PyValueError::new_err)?;
    Ok(doc)
}

/// measure text width without a page context.
#[pyfunction]
fn text_width(text: &str, font: &str, size: f64) -> PyResult<f64> {
    let width = font_metrics::measure_str(text, font, size as f32)
        .ok_or_else(|| PyValueError::new_err(format!("unknown font: {font}")))?;
    Ok(f64::from(width))
}

/// word-wrap text into lines that fit within `max_width` points.
#[pyfunction]
fn wrap_text(text: &str, max_width: f64, font: &str, font_size: f64) -> PyResult<Vec<String>> {
    layout::wrap_text_impl(text, max_width as f32, font, font_size as f32)
        .map_err(PyValueError::new_err)
}

// ── Module ─────────────────────────────────────────────────────

#[pymodule]
mod _core {
    use pyo3::prelude::*;

    use super::{
        html_to_pdf, layout, text_width, wrap_text, FontDatabase, FontId, Page, PdfDocument,
    };

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PdfDocument>()?;
        m.add_class::<Page>()?;
        m.add_class::<FontDatabase>()?;
        m.add_class::<FontId>()?;
        m.add_class::<layout::Layout>()?;
        m.add_class::<layout::TextRun>()?;
        m.add_function(wrap_pyfunction!(text_width, m)?)?;
        m.add_function(wrap_pyfunction!(wrap_text, m)?)?;
        m.add_function(wrap_pyfunction!(html_to_pdf, m)?)?;
        Ok(())
    }
}
