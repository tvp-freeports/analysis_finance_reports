pub mod pdf_line;
pub mod relative;

use pyo3::prelude::*;
use pyo3::types::{PyList};

use crate::commons::sets::Container;
use relative::{
    RelativePdfLineSet,
    OptionallyRelative,
    RelativeInfo,
    RelativeSelectPdfLineSet
};
use pdf_line::{PdfLine,PdfLineSet,SelectPdfLineSet};

#[pyclass]
#[pyo3(name = "PdfLineSelection")]
#[derive(Clone)]
pub struct PyPdfLineSelection(RelativePdfLineSet);


#[pyclass]
#[pyo3(name = "PdfLineSet")]
pub struct PyPdfLineSet(PdfLineSet);

#[pyclass]
#[pyo3(name = "PdfLine")]
#[derive(Clone)]
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
            "PdfLine(font=\"{font}\",font_size={font_size},text=\"{text}\",bbox=({x0},{y0},{x1},{y1}))",
            font=l.font().inner(),
            font_size=l.font_size(),
            text=l.text(),
            x0=a.0,y0=a.1,x1=a.2,y1=a.3
        )
    }
}


#[pymethods]
impl PyPdfLineSelection {
    #[new]
    #[pyo3(signature = (font=None,font_size=None,text=None,area=None))]
    fn new(font: Option<&str>, font_size: Option<(f32,f32)>, text: Option<&str>, area: Option<(f32,f32,f32,f32)>) -> Self {
        match (font,font_size,text,area) {
            (None,None,None,None) => Self::font_size(0.0,1e6),
            (Some(f),None,None,None) => Self::font(f),
            (None,Some((a,b)),None,None) => Self::font_size(a,b),
            (None,None,Some(t),None) => Self::text(t),
            (None,None,None,Some((x0,y0,x1,y1))) => Self::area(x0,y0,x1,y1),
            (Some(f),Some((a,b)),None,None) => Self::font(f).__and__(Self::font_size(a,b)),
            (Some(f),None,Some(t),None) => Self::font(f).__and__(Self::text(t)),
            (Some(f),None,None,Some((x0,y0,x1,y1))) => Self::font(f).__and__(Self::area(x0,y0,x1,y1)),
            (None,Some((a,b)),Some(t),None) => Self::font_size(a,b).__and__(Self::text(t)),
            (None,Some((a,b)),None,Some((x0,y0,x1,y1))) => Self::font_size(a,b).__and__(Self::area(x0,y0,x1,y1)),
            (None,None,Some(t),Some((x0,y0,x1,y1))) => Self::text(t).__and__(Self::area(x0,y0,x1,y1)),
            (Some(f),Some((a,b)),Some(t),None) => Self::font(f).__and__(Self::font_size(a,b)).__and__(Self::text(t)),
            (Some(f),Some((a,b)),None,Some((x0,y0,x1,y1))) => Self::font(f).__and__(Self::font_size(a,b)).__and__(Self::area(x0,y0,x1,y1)),
            (Some(f),None,Some(t),Some((x0,y0,x1,y1))) => Self::font(f).__and__(Self::text(t)).__and__(Self::area(x0,y0,x1,y1)),
            (None,Some((a,b)),Some(t),Some((x0,y0,x1,y1))) => Self::font_size(a,b).__and__(Self::text(t)).__and__(Self::area(x0,y0,x1,y1)),
            (Some(f),Some((a,b)),Some(t),Some((x0,y0,x1,y1))) => {
                Self::font(f).__and__(Self::font_size(a,b)).__and__(Self::text(t)).__and__(Self::area(x0,y0,x1,y1))
            }
        }
    }
    #[staticmethod]
    fn font_of(target: Self) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Relative(
                    RelativeSelectPdfLineSet::select_font_of(Relative(target.0))
                )
            )
        )
    }
    #[staticmethod]
    fn font_size_of(target: Self) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Relative(
                    RelativeSelectPdfLineSet::select_fontsize_of(Relative(target.0))
                )
            )
        )
    }
    #[staticmethod]
    fn text_of(target: Self) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Relative(
                    RelativeSelectPdfLineSet::select_text_of(Relative(target.0))
                )
            )
        )
    }
    #[staticmethod]
    fn area_of(target: Self) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Relative(
                    RelativeSelectPdfLineSet::select_area_of(Relative(target.0))
                )
            )
        )
    }
    #[staticmethod]
    fn font(font: &str) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Absolute(SelectPdfLineSet::select_font(font))
            )
        )
    }
    #[staticmethod]
    fn font_size(a: f32, b: f32) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Absolute(SelectPdfLineSet::select_fontsize(a,b))
            )
        )
    }
    #[staticmethod]
    fn text(text: &str) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Absolute(SelectPdfLineSet::select_text(text))
            )
        )
    }
    #[staticmethod]
    fn area(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        use OptionallyRelative::*;
        Self(
            RelativePdfLineSet::from_leaf(
                Absolute(SelectPdfLineSet::select_area(x0,y0,x1,y1))
            )
        )
    }
    fn __or__(&self,other: Self) -> Self {
        Self(self.0.clone() | other.0.clone())
    }
    fn __and__(&self,other: Self) -> Self {
        Self(self.0.clone() & other.0.clone())
    }
    fn __truediv__(&self,other: Self) -> Self {
        Self(self.0.clone() / other.0.clone())
    }
    fn __add__(&self,other: Self) -> Self {
        self.__or__(other)
    }
    fn __sub__(&self,other: Self) -> Self {
        self.__and__(other)
    }
    fn contextualize<'py>(&self,py_lines: &Bound<'py,PyList>) -> PyResult<PyPdfLineSet> {
        let lin: Vec<PyPdfLine> = py_lines.extract()?;
        let lines: Vec<PdfLine> = lin.into_iter().map(|a| a.0).collect();
        Ok(PyPdfLineSet(self.0.clone().contextualize(&lines)))
    }
    fn select<'py>(&self, py: Python<'py>, py_lines: &Bound<'py,PyList>) -> PyResult<Bound<'py, PyList>> {
        let lin: Vec<PyPdfLine> = py_lines.extract()?;
        let lines: Vec<PdfLine> = lin.into_iter().map(|a| a.0).collect();
        let set=self.0.clone().contextualize(&lines);
        let res: Vec<PyPdfLine> = lines.into_iter()
        .filter(|l| set.contains(&l))
        .map(|l| PyPdfLine(l)).collect();
        PyList::new(py,res)
    }
}


#[pymethods]
impl PyPdfLineSet {
    fn __contains__(&self,line: PyPdfLine) -> PyResult<bool> {
        Ok(self.0.contains(&line.0))
    }

}


