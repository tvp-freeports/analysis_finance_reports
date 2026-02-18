pub mod pdf_line;
pub mod relative;

use pyo3::prelude::*;

pub use relative::PdfLineSelection;
pub use pdf_line::PdfLine;

#[pyclass]
#[pyo3(name = "PdfLineSelection")]
pub struct PyPdfLineSelection(PdfLineSelection);


#[pyclass]
#[pyo3(name = "PdfLine")]
pub struct PyPdfLine(PdfLine);

#[pymethods]
impl PyPdfLine {
    #[new]
    fn new(font: &str, font_size: f32, text: &str, area: (f32,f32,f32,f32)) -> Self {
        Self(PdfLine::new(font,font_size,text,area))
    }
    fn __repr__(&self) -> String {
        let PyPdfLine(l) = self;
        let a = l.bbox().as_tuple();
        format!(
            "PdfLine{{{font}[{font_size}]\"{text}\"(({x0}:{x1})({y0}:{y1}))}}",
            font=l.font().inner(),
            font_size=l.font_size(),
            text=l.text(),
            x0=a.0,y0=a.1,x1=a.2,y1=a.3
        )
    }
}