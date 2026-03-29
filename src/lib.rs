use std::sync::{Arc, Mutex};

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};
use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::prelude::*;

mod font_metrics;

// ── Built-in PDF fonts ─────────────────────────────────────────

const BUILTIN_FONTS: &[&str] = &[
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

enum PdfOp {
    BeginText,
    EndText,
    SetFont { name: String, size: f32 },
    SetTextPosition { x: f32, y: f32 },
    ShowText(String),
}

struct PageContent {
    width: f64,
    height: f64,
    operations: Vec<PdfOp>,
    fonts_used: Vec<String>,
    current_font: Option<String>,
    current_font_size: Option<f32>,
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

// ── PdfDocument ────────────────────────────────────────────────

#[pyclass]
struct PdfDocument {
    pages: Vec<Arc<Mutex<PageContent>>>,
}

#[pymethods]
impl PdfDocument {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(PdfDocument { pages: Vec::new() })
    }

    #[pyo3(signature = (width=612.0, height=792.0))]
    fn add_page(&mut self, width: f64, height: f64) -> PyResult<Page> {
        let content = Arc::new(Mutex::new(PageContent::new(width, height)));
        self.pages.push(Arc::clone(&content));
        Ok(Page { content })
    }

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

        // Pre-allocate refs for each page (page obj + content stream)
        let page_refs: Vec<(Ref, Ref)> = self.pages.iter().map(|_| (alloc(), alloc())).collect();

        // Collect all unique font names across all pages
        let mut all_fonts: Vec<String> = Vec::new();
        for page_arc in &self.pages {
            let page = page_arc.lock().unwrap();
            for font_name in &page.fonts_used {
                if !all_fonts.contains(font_name) {
                    all_fonts.push(font_name.clone());
                }
            }
        }

        // Allocate font refs
        let font_refs: Vec<(String, Ref)> = all_fonts
            .iter()
            .map(|name| (name.clone(), alloc()))
            .collect();

        // Write catalog
        pdf.catalog(catalog_id).pages(page_tree_id);

        // Write page tree
        let page_ids: Vec<Ref> = page_refs.iter().map(|(pid, _)| *pid).collect();
        pdf.pages(page_tree_id)
            .kids(page_ids)
            .count(self.pages.len() as i32);

        // Write each page
        for (i, page_arc) in self.pages.iter().enumerate() {
            let page = page_arc.lock().unwrap();
            let (page_id, content_id) = page_refs[i];

            // Build content stream
            let mut content = Content::new();
            for op in &page.operations {
                match op {
                    PdfOp::BeginText => {
                        content.begin_text();
                    }
                    PdfOp::EndText => {
                        content.end_text();
                    }
                    PdfOp::SetFont { name, size } => {
                        // Find the font resource name (F1, F2, etc.)
                        let idx = font_refs.iter().position(|(n, _)| n == name).unwrap_or(0);
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
                }
            }

            let content_bytes = content.finish();

            // Write page object
            {
                let mut page_writer = pdf.page(page_id);
                page_writer
                    .parent(page_tree_id)
                    .media_box(Rect::new(0.0, 0.0, page.width as f32, page.height as f32))
                    .contents(content_id);

                // Add font resources — must bind resources() to extend its lifetime
                let mut resources = page_writer.resources();
                let mut fonts_dict = resources.fonts();
                for (idx, (_, font_ref)) in font_refs.iter().enumerate() {
                    let resource_name = format!("F{}", idx + 1);
                    fonts_dict.pair(Name(resource_name.as_bytes()), *font_ref);
                }
            }

            // Write content stream
            pdf.stream(content_id, &content_bytes);
        }

        // Write font objects
        for (font_name, font_ref) in &font_refs {
            pdf.type1_font(*font_ref)
                .base_font(Name(font_name.as_bytes()));
        }

        Ok(pdf.finish())
    }

    fn save(&self, path: &str) -> PyResult<()> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)
            .map_err(|e| PyValueError::new_err(format!("failed to write file: {e}")))?;
        Ok(())
    }

    #[allow(unused_variables)]
    fn register_font(&mut self, font_db: &FontDatabase, font_id: &FontId) -> PyResult<String> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
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
        if !BUILTIN_FONTS.contains(&name) {
            return Err(PyValueError::new_err(format!(
                "unknown built-in font: {name}"
            )));
        }
        let mut page = self.content.lock().unwrap();
        // Track that this page uses this font
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
        let metrics = font_metrics::get_builtin_metrics(font_name)
            .ok_or_else(|| PyValueError::new_err(format!("no metrics for font: {font_name}")))?;
        let width: u32 = text.bytes().map(|b| metrics.widths[b as usize] as u32).sum();
        Ok(width as f64 * font_size as f64 / 1000.0)
    }

    fn draw_text(&mut self, x: f64, y: f64, text: &str) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetTextPosition {
            x: x as f32,
            y: y as f32,
        });
        page.operations.push(PdfOp::ShowText(text.to_string()));
        page.operations.push(PdfOp::EndText);
        Ok(())
    }
}

// ── FontDatabase (stub) ────────────────────────────────────────

#[pyclass]
struct FontDatabase;

#[pymethods]
#[allow(unused_variables)]
impl FontDatabase {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(FontDatabase)
    }

    fn load_system_fonts(&mut self) -> PyResult<()> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
    }

    fn load_font_file(&mut self, path: &str) -> PyResult<FontId> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
    }

    fn load_font_data(&mut self, data: &[u8]) -> PyResult<FontId> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
    }

    #[pyo3(signature = (family, weight=None, italic=None))]
    fn query(
        &self,
        family: &str,
        weight: Option<u16>,
        italic: Option<bool>,
    ) -> PyResult<Option<FontId>> {
        Err(PyNotImplementedError::new_err("not yet implemented"))
    }
}

// ── FontId (stub) ──────────────────────────────────────────────

#[pyclass]
struct FontId;

#[pymethods]
impl FontId {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(FontId)
    }
}

/// measure text width without a page context.
#[pyfunction]
fn text_width(text: &str, font: &str, size: f64) -> PyResult<f64> {
    let metrics = font_metrics::get_builtin_metrics(font)
        .ok_or_else(|| PyValueError::new_err(format!("unknown font: {font}")))?;
    let width: u32 = text.bytes().map(|b| metrics.widths[b as usize] as u32).sum();
    Ok(width as f64 * size / 1000.0)
}

// ── Module ─────────────────────────────────────────────────────

#[pymodule]
mod _core {
    use super::*;

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PdfDocument>()?;
        m.add_class::<Page>()?;
        m.add_class::<FontDatabase>()?;
        m.add_class::<FontId>()?;
        m.add_function(wrap_pyfunction!(text_width, m)?)?;
        Ok(())
    }
}
