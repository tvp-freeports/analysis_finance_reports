//! A typed view of PyMuPDF's page dict, and the pure functions that derive [`PdfLine`]s and
//! [`PageImage`]s from it.
//!
//! The parsing from Python lives in one place (`PageDict::from_py`); everything else here is
//! ordinary Rust over ordinary data, and therefore testable without an interpreter.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::commons::geometry::Rectangle;
use crate::core::page::{PageError, PageImage};
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

#[derive(Debug, Clone, PartialEq)]
pub struct PageDictSpan {
    pub font: String,
    pub size: f32,
    pub text: String,
    pub bbox: (f32, f32, f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageDictLine {
    pub dir: (f32, f32),
    pub bbox: (f32, f32, f32, f32),
    pub spans: Vec<PageDictSpan>,
}

/// A block of the page dict. Vector-image blocks, and any other unhandled type, become `Other` and
/// are ignored.
#[derive(Debug, Clone, PartialEq)]
pub enum PageDictBlock {
    Text { lines: Vec<PageDictLine> },
    ImageRaster { bbox: (f32, f32, f32, f32), ext: String, data: Vec<u8> },
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageDict {
    pub width: f32,
    pub height: f32,
    pub blocks: Vec<PageDictBlock>,
}

/// The span-collapsing threshold.
const SPAN_COLLAPSE_THRESHOLD: f32 = 1e-1;

/// Collapses a line's spans into one when the font and size are (nearly) constant along it.
///
/// An empty span list yields an empty vector rather than a panic or a `NaN` average.
pub(crate) fn collapse_spans_from_line(line: &PageDictLine, threshold: f32) -> Vec<PageDictSpan> {
    if line.spans.is_empty() {
        return Vec::new();
    }

    let mut collapse = true;
    let mut sum_font_size = 0.0f32;
    let mut text = String::new();
    let mut last_font: Option<&str> = None;
    let mut last_size: Option<f32> = None;
    let mut res = Vec::with_capacity(line.spans.len());

    for s in &line.spans {
        sum_font_size += s.size;
        text.push_str(&s.text);
        if let (Some(lf), Some(ls)) = (last_font, last_size)
            && (s.font != lf || (s.size - ls).abs() > threshold)
        {
            collapse = false;
        }
        last_font = Some(&s.font);
        last_size = Some(s.size);
        res.push(s.clone());
    }

    if collapse {
        // `line.spans` is non-empty here, checked above, so the loop ran at least once.
        let font = last_font.expect("line.spans is non-empty, so the loop above ran at least once").to_string();
        vec![PageDictSpan { font, size: sum_font_size / line.spans.len() as f32, text, bbox: line.bbox }]
    } else {
        res
    }
}

/// Rotates and translates a bounding box, given the cosine and sine of the angle: the minimum and
/// maximum over the four rotated corners, then the translation.
pub(crate) fn rotate_bbox(bbox: (f32, f32, f32, f32), cs: f32, sn: f32, new_left: f32, new_top: f32) -> (f32, f32, f32, f32) {
    let (x0, y0, x1, y1) = bbox;
    let corners = [(x0, y0), (x0, y1), (x1, y1), (x1, y0)];
    let xs = corners.map(|(x, y)| cs * x + sn * y);
    let ys = corners.map(|(x, y)| cs * y - sn * x);
    let new_x0 = xs.into_iter().fold(f32::INFINITY, f32::min);
    let new_x1 = xs.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let new_y0 = ys.into_iter().fold(f32::INFINITY, f32::min);
    let new_y1 = ys.into_iter().fold(f32::NEG_INFINITY, f32::max);
    (new_x0 - new_left, new_y0 - new_top, new_x1 - new_left, new_y1 - new_top)
}

/// Leaves a line unchanged when it is already horizontal; otherwise computes the rotation origin
/// from the page's corners and rotates the line's box and every span's, resetting the direction to
/// horizontal.
pub(crate) fn rotate_line(line: &PageDictLine, width: f32, height: f32) -> PageDictLine {
    let (c, s) = line.dir;
    if c == 1.0 && s == 0.0 {
        return line.clone();
    }

    let corners = [(0.0, 0.0), (0.0, height), (width, height), (width, 0.0)];
    let new_left = corners.iter().map(|&(x, y)| c * x + s * y).fold(f32::INFINITY, f32::min);
    let new_top = corners.iter().map(|&(x, y)| c * y - s * x).fold(f32::INFINITY, f32::min);

    let bbox = rotate_bbox(line.bbox, c, s, new_left, new_top);
    let spans = line
        .spans
        .iter()
        .map(|span| PageDictSpan { bbox: rotate_bbox(span.bbox, c, s, new_left, new_top), ..span.clone() })
        .collect();

    PageDictLine { dir: (1.0, 0.0), bbox, spans }
}

/// Extracts the text lines, rotates them if asked, collapses their spans, and builds the resulting
/// [`PdfLine`]s.
///
/// # Why degenerate spans are dropped rather than propagated
///
/// [`PdfLine::new`] and [`Rectangle::new`] panic on a non-positive font size or an inverted box,
/// which is right for values built inside the crate. This function is the first place those
/// constructors are reachable from a **real** PyMuPDF dict, that is, from untrusted input: a
/// malformed PDF must not be able to abort the process.
///
/// So a span with a non-positive size or an inverted box is dropped, with a warning so the loss is
/// visible. The test comparing coordinates uses `<` rather than `!=`, which excludes the degenerate
/// case and the inverted one in a single check.
pub fn pdflines_from_pagedict(page: &PageDict, auto_rotate: bool) -> Vec<PdfLine> {
    let source_lines: Vec<&PageDictLine> = page
        .blocks
        .iter()
        .filter_map(|b| match b {
            PageDictBlock::Text { lines } => Some(lines),
            _ => None,
        })
        .flatten()
        .collect();

    let lines: Vec<PageDictLine> = if auto_rotate {
        source_lines.iter().map(|l| rotate_line(l, page.width, page.height)).collect()
    } else {
        source_lines.into_iter().cloned().collect()
    };

    lines
        .iter()
        .flat_map(|l| collapse_spans_from_line(l, SPAN_COLLAPSE_THRESHOLD))
        .filter(|s| {
            let (x0, y0, x1, y1) = s.bbox;
            if s.size <= 0.0 {
                tracing::warn!(
                    coord_ref_1 = %s.text,
                    size = s.size,
                    "discarding a pdf span with non-positive font size"
                );
                return false;
            }
            if !(x0 < x1 && y0 < y1) {
                tracing::warn!(
                    coord_ref_1 = %s.text,
                    coord_1 = %format!("x {x0}..{x1}"),
                    coord_2 = %format!("y {y0}..{y1}"),
                    "discarding a pdf span with a degenerate or inverted bbox"
                );
                return false;
            }
            true
        })
        .map(|s| PdfLine::new(&s.font, s.size, &s.text, s.bbox))
        .collect()
}

/// Extracts the raster images as [`PageImage`]s: raw, undecoded bytes.
pub fn pdfimages_from_pagedict(page: &PageDict) -> Vec<PageImage> {
    page.blocks
        .iter()
        .filter_map(|b| match b {
            PageDictBlock::ImageRaster { bbox, ext, data } => {
                let (x0, y0, x1, y1) = *bbox;
                Some(PageImage { bbox: Rectangle::new(x0, y0, x1, y1), ext: ext.clone(), data: data.clone() })
            }
            _ => None,
        })
        .collect()
}

fn parse_fail(message: String) -> PageError {
    PageError::ParseFail { message }
}

fn line_parse_fail(message: String) -> PageError {
    PageError::LineParseFail { message }
}

/// Reads `key` from `dict`, mapping both a missing key and a lookup error onto the same error
/// constructor.
fn dict_get<'py>(dict: &Bound<'py, PyDict>, key: &str, err: fn(String) -> PageError) -> Result<Bound<'py, PyAny>, PageError> {
    dict.get_item(key)
        .map_err(|e| err(format!("could not look up '{key}': {e}")))?
        .ok_or_else(|| err(format!("missing key '{key}'")))
}

fn extract_f32(value: &Bound<'_, PyAny>, field: &str, err: fn(String) -> PageError) -> Result<f32, PageError> {
    value.extract::<f64>().map(|v| v as f32).map_err(|_| err(format!("'{field}' is not a number")))
}

fn extract_string(value: &Bound<'_, PyAny>, field: &str, err: fn(String) -> PageError) -> Result<String, PageError> {
    value.extract::<String>().map_err(|_| err(format!("'{field}' is not a string")))
}

fn extract_bytes(value: &Bound<'_, PyAny>, field: &str, err: fn(String) -> PageError) -> Result<Vec<u8>, PageError> {
    value.extract::<Vec<u8>>().map_err(|_| err(format!("'{field}' is not bytes")))
}

fn extract_pair(dict: &Bound<'_, PyDict>, key: &str, err: fn(String) -> PageError) -> Result<(f32, f32), PageError> {
    let value = dict_get(dict, key, err)?;
    let seq: Vec<f64> = value.extract().map_err(|_| err(format!("'{key}' is not a 2-tuple of numbers")))?;
    match seq.as_slice() {
        [a, b] => Ok((*a as f32, *b as f32)),
        _ => Err(err(format!("'{key}' must have exactly 2 numbers, found {}", seq.len()))),
    }
}

fn extract_quad(dict: &Bound<'_, PyDict>, key: &str, err: fn(String) -> PageError) -> Result<(f32, f32, f32, f32), PageError> {
    let value = dict_get(dict, key, err)?;
    let seq: Vec<f64> = value.extract().map_err(|_| err(format!("'{key}' is not a 4-tuple of numbers")))?;
    match seq.as_slice() {
        [a, b, c, d] => Ok((*a as f32, *b as f32, *c as f32, *d as f32)),
        _ => Err(err(format!("'{key}' must have exactly 4 numbers, found {}", seq.len()))),
    }
}

fn parse_span(span: &Bound<'_, PyAny>) -> Result<PageDictSpan, PageError> {
    let span_dict = span.cast::<PyDict>().map_err(|_| line_parse_fail("a span entry is not a dict".to_string()))?;
    let font = extract_string(&dict_get(span_dict, "font", line_parse_fail)?, "font", line_parse_fail)?;
    let size = extract_f32(&dict_get(span_dict, "size", line_parse_fail)?, "size", line_parse_fail)?;
    let text = extract_string(&dict_get(span_dict, "text", line_parse_fail)?, "text", line_parse_fail)?;
    let bbox = extract_quad(span_dict, "bbox", line_parse_fail)?;
    Ok(PageDictSpan { font, size, text, bbox })
}

fn parse_line(line: &Bound<'_, PyAny>) -> Result<PageDictLine, PageError> {
    let line_dict = line.cast::<PyDict>().map_err(|_| line_parse_fail("a line entry is not a dict".to_string()))?;
    let dir = extract_pair(line_dict, "dir", line_parse_fail)?;
    let bbox = extract_quad(line_dict, "bbox", line_parse_fail)?;
    let spans_obj = dict_get(line_dict, "spans", line_parse_fail)?;
    let spans_list = spans_obj.cast::<PyList>().map_err(|_| line_parse_fail("'spans' is not a list".to_string()))?;
    let spans = spans_list.iter().map(|s| parse_span(&s)).collect::<Result<Vec<_>, _>>()?;
    Ok(PageDictLine { dir, bbox, spans })
}

fn parse_block(block: &Bound<'_, PyDict>) -> Result<PageDictBlock, PageError> {
    let type_val = dict_get(block, "type", parse_fail)?;
    let block_type: i64 = type_val.extract().map_err(|_| parse_fail("'type' is not an integer".to_string()))?;
    match block_type {
        0 => {
            // A text block with no lines key is not an error: it becomes an empty text block.
            let lines = match block.get_item("lines").map_err(|e| parse_fail(format!("could not look up 'lines': {e}")))? {
                None => Vec::new(),
                Some(lines_obj) => {
                    let lines_list = lines_obj.cast::<PyList>().map_err(|_| parse_fail("'lines' is not a list".to_string()))?;
                    lines_list.iter().map(|l| parse_line(&l)).collect::<Result<Vec<_>, _>>()?
                }
            };
            Ok(PageDictBlock::Text { lines })
        }
        1 => {
            let bbox = extract_quad(block, "bbox", parse_fail)?;
            let ext = extract_string(&dict_get(block, "ext", parse_fail)?, "ext", parse_fail)?;
            let data = extract_bytes(&dict_get(block, "image", parse_fail)?, "image", parse_fail)?;
            Ok(PageDictBlock::ImageRaster { bbox, ext, data })
        }
        _ => {
            // Not an error, and not necessarily lost work either — vector graphics and logos are
            // ignored by design and are far too common in real PDFs to warrant a warning.
            tracing::trace!(block_type, "unhandled block type, treated as Other");
            Ok(PageDictBlock::Other)
        }
    }
}

impl PageDict {
    /// The PyO3 boundary: extracts a [`PageDict`] from a dict shaped like PyMuPDF's page text
    /// output.
    ///
    /// # Errors
    ///
    /// [`PageError::ParseFail`] for an unexpected shape at page or block level,
    /// [`PageError::LineParseFail`] for one inside a line or span.
    pub(crate) fn from_py(dict: &Bound<'_, PyDict>) -> Result<Self, PageError> {
        let width = extract_f32(&dict_get(dict, "width", parse_fail)?, "width", parse_fail)?;
        let height = extract_f32(&dict_get(dict, "height", parse_fail)?, "height", parse_fail)?;
        let blocks_obj = dict_get(dict, "blocks", parse_fail)?;
        let blocks_list = blocks_obj.cast::<PyList>().map_err(|_| parse_fail("'blocks' is not a list".to_string()))?;
        let blocks = blocks_list
            .iter()
            .map(|b| {
                let block_dict = b.cast::<PyDict>().map_err(|_| parse_fail("a block entry is not a dict".to_string()))?;
                parse_block(block_dict)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PageDict { width, height, blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(font: &str, size: f32, text: &str, bbox: (f32, f32, f32, f32)) -> PageDictSpan {
        PageDictSpan { font: font.to_string(), size, text: text.to_string(), bbox }
    }

    fn line(dir: (f32, f32), bbox: (f32, f32, f32, f32), spans: Vec<PageDictSpan>) -> PageDictLine {
        PageDictLine { dir, bbox, spans }
    }

    /// Rotation is tested in isolation from its real callers. The expected values were computed by
    /// reproducing the rotation formula outside this crate, not guessed.
    mod rotate_bbox_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn identity_rotation_with_no_translation_leaves_the_bbox_unchanged() {
            let bbox = (0.0, 0.0, 10.0, 20.0);
            assert_eq!(rotate_bbox(bbox, 1.0, 0.0, 0.0, 0.0), bbox);
        }

        #[test]
        fn rotates_ninety_degrees_with_no_translation() {
            let bbox = (0.0, 0.0, 10.0, 20.0);
            assert_eq!(rotate_bbox(bbox, 0.0, 1.0, 0.0, 0.0), (0.0, -10.0, 20.0, 0.0));
        }

        #[test]
        fn rotates_one_hundred_eighty_degrees_with_no_translation() {
            let bbox = (0.0, 0.0, 10.0, 20.0);
            assert_eq!(rotate_bbox(bbox, -1.0, 0.0, 0.0, 0.0), (-10.0, -20.0, 0.0, 0.0));
        }

        #[test]
        fn rotates_two_hundred_seventy_degrees_with_no_translation() {
            let bbox = (0.0, 0.0, 10.0, 20.0);
            assert_eq!(rotate_bbox(bbox, 0.0, -1.0, 0.0, 0.0), (-20.0, 0.0, 0.0, 10.0));
        }

        #[test]
        fn identity_rotation_still_applies_the_translation() {
            let bbox = (0.0, 0.0, 10.0, 20.0);
            assert_eq!(rotate_bbox(bbox, 1.0, 0.0, 5.0, 3.0), (-5.0, -3.0, 5.0, 17.0));
        }
    }

    mod rotate_line_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_horizontal_line_is_returned_unchanged() {
            let l = line(
                (1.0, 0.0),
                (1.0, 2.0, 3.0, 4.0),
                vec![span("Arial", 10.0, "hi", (1.0, 2.0, 3.0, 4.0))],
            );
            assert_eq!(rotate_line(&l, 100.0, 200.0), l);
        }

        /// Expected values computed outside this crate: a 100x200 page, a quarter turn, and one
        /// line with two spans covering half the box each.
        #[test]
        fn rotates_the_line_bbox_and_every_span_bbox_with_the_same_origin_and_zeroes_dir() {
            let l = line(
                (0.0, 1.0),
                (10.0, 10.0, 20.0, 30.0),
                vec![
                    span("Arial", 10.0, "a", (10.0, 10.0, 15.0, 30.0)),
                    span("Arial", 10.0, "b", (15.0, 10.0, 20.0, 30.0)),
                ],
            );
            let rotated = rotate_line(&l, 100.0, 200.0);

            assert_eq!(rotated.dir, (1.0, 0.0));
            assert_eq!(rotated.bbox, (10.0, 80.0, 30.0, 90.0));
            assert_eq!(rotated.spans.len(), 2);
            assert_eq!(rotated.spans[0].bbox, (10.0, 85.0, 30.0, 90.0));
            assert_eq!(rotated.spans[1].bbox, (10.0, 80.0, 30.0, 85.0));
            // Font, size and text are untouched by rotation.
            assert_eq!(rotated.spans[0].font, "Arial");
            assert_eq!(rotated.spans[0].text, "a");
            assert_eq!(rotated.spans[1].text, "b");
        }
    }

    mod collapse_spans_from_line_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_single_span_passes_through_unchanged() {
            let l = line((1.0, 0.0), (0.0, 0.0, 10.0, 10.0), vec![span("Arial", 10.0, "hi", (0.0, 0.0, 10.0, 10.0))]);
            let out = collapse_spans_from_line(&l, 0.1);
            assert_eq!(out, vec![span("Arial", 10.0, "hi", (0.0, 0.0, 10.0, 10.0))]);
        }

        #[test]
        fn collapses_same_font_spans_within_threshold_concatenating_text_averaging_size_and_using_the_lines_own_bbox() {
            // The line's box is deliberately different from every individual span's, to show the
            // result uses the line's box rather than the first or last span's.
            let sizes = [10.0_f32, 10.02, 10.05];
            let l = line(
                (1.0, 0.0),
                (0.0, 0.0, 100.0, 10.0),
                vec![
                    span("Arial", sizes[0], "foo", (0.0, 0.0, 30.0, 10.0)),
                    span("Arial", sizes[1], "bar", (30.0, 0.0, 60.0, 10.0)),
                    span("Arial", sizes[2], "baz", (60.0, 0.0, 100.0, 10.0)),
                ],
            );
            let out = collapse_spans_from_line(&l, 0.1);

            // The same sum-then-divide as the implementation, accumulating in span order, to avoid
            // rounding fragility between test and code.
            let mut sum = 0.0_f32;
            for s in sizes {
                sum += s;
            }
            let expected_avg = sum / sizes.len() as f32;

            assert_eq!(out, vec![span("Arial", expected_avg, "foobarbaz", (0.0, 0.0, 100.0, 10.0))]);
        }

        #[test]
        fn does_not_collapse_spans_with_different_fonts() {
            let spans = vec![span("Arial", 10.0, "foo", (0.0, 0.0, 5.0, 10.0)), span("Times", 10.0, "bar", (5.0, 0.0, 10.0, 10.0))];
            let l = line((1.0, 0.0), (0.0, 0.0, 10.0, 10.0), spans.clone());
            assert_eq!(collapse_spans_from_line(&l, 0.1), spans);
        }

        #[test]
        fn does_not_collapse_when_the_size_difference_exceeds_the_threshold() {
            let spans = vec![span("Arial", 10.0, "foo", (0.0, 0.0, 5.0, 10.0)), span("Arial", 10.5, "bar", (5.0, 0.0, 10.0, 10.0))];
            let l = line((1.0, 0.0), (0.0, 0.0, 10.0, 10.0), spans.clone());
            assert_eq!(collapse_spans_from_line(&l, 0.1), spans);
        }

        #[test]
        fn collapses_when_the_size_difference_exactly_equals_the_threshold() {
            // 0.5 and the two sizes are exactly representable, so the difference is *exactly* the
            // threshold, bit for bit, and not merely close to it — a deliberate pin of the
            // boundary, where equal to the threshold still collapses.
            let threshold = 0.5_f32;
            let l = line(
                (1.0, 0.0),
                (0.0, 0.0, 10.0, 10.0),
                vec![span("Arial", 10.0, "foo", (0.0, 0.0, 5.0, 10.0)), span("Arial", 10.5, "bar", (5.0, 0.0, 10.0, 10.0))],
            );
            let out = collapse_spans_from_line(&l, threshold);
            assert_eq!(out, vec![span("Arial", 10.25, "foobar", (0.0, 0.0, 10.0, 10.0))]);
        }

        #[test]
        fn does_not_collapse_when_the_size_difference_is_just_over_the_threshold() {
            let threshold = 0.5_f32;
            let spans = vec![span("Arial", 10.0, "foo", (0.0, 0.0, 5.0, 10.0)), span("Arial", 10.75, "bar", (5.0, 0.0, 10.0, 10.0))];
            let l = line((1.0, 0.0), (0.0, 0.0, 10.0, 10.0), spans.clone());
            assert_eq!(collapse_spans_from_line(&l, threshold), spans);
        }

        /// No panic, no `NaN`: an empty vector.
        #[test]
        fn a_line_with_no_spans_returns_an_empty_vec() {
            let l = line((1.0, 0.0), (0.0, 0.0, 10.0, 10.0), vec![]);
            assert_eq!(collapse_spans_from_line(&l, 0.1), Vec::<PageDictSpan>::new());
        }
    }

    mod pdflines_from_pagedict_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        fn accessors(l: &PdfLine) -> (String, f32, String, (f32, f32, f32, f32)) {
            (l.font().inner().to_string(), *l.font_size(), l.text().clone(), l.bbox().as_tuple())
        }

        #[test]
        fn non_text_blocks_are_ignored() {
            let page = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Other, PageDictBlock::ImageRaster { bbox: (0.0, 0.0, 1.0, 1.0), ext: "png".to_string(), data: vec![] }],
            };
            assert!(pdflines_from_pagedict(&page, false).is_empty());
        }

        #[test]
        fn multiple_text_blocks_are_concatenated_in_order() {
            let page = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![
                    PageDictBlock::Text {
                        lines: vec![line((1.0, 0.0), (0.0, 0.0, 10.0, 10.0), vec![span("Arial", 10.0, "first", (0.0, 0.0, 10.0, 10.0))])],
                    },
                    PageDictBlock::Text {
                        lines: vec![line((1.0, 0.0), (0.0, 20.0, 10.0, 30.0), vec![span("Arial", 10.0, "second", (0.0, 20.0, 10.0, 30.0))])],
                    },
                ],
            };
            let lines = pdflines_from_pagedict(&page, false);
            let texts: Vec<String> = lines.iter().map(|l| l.text().clone()).collect();
            assert_eq!(texts, vec!["first".to_string(), "second".to_string()]);
        }

        #[test]
        fn auto_rotate_true_rotates_non_horizontal_lines_before_collapsing() {
            let page = PageDict {
                width: 100.0,
                height: 200.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((0.0, 1.0), (10.0, 10.0, 20.0, 30.0), vec![span("Arial", 10.0, "rotated", (10.0, 10.0, 20.0, 30.0))])],
                }],
            };
            let lines = pdflines_from_pagedict(&page, true);
            assert_eq!(lines.len(), 1);
            // The same formula as in the rotation tests, for a box of (10,10,20,30), a quarter turn
            // and a 100x200 page, giving an origin of (0, -100).
            assert_eq!(accessors(&lines[0]), ("arial".to_string(), 10.0, "rotated".to_string(), (10.0, 80.0, 30.0, 90.0)));
        }

        #[test]
        fn auto_rotate_false_leaves_non_horizontal_lines_as_is() {
            let page = PageDict {
                width: 100.0,
                height: 200.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((0.0, 1.0), (10.0, 10.0, 20.0, 30.0), vec![span("Arial", 10.0, "not-rotated", (10.0, 10.0, 20.0, 30.0))])],
                }],
            };
            let lines = pdflines_from_pagedict(&page, false);
            assert_eq!(lines.len(), 1);
            assert_eq!(accessors(&lines[0]), ("arial".to_string(), 10.0, "not-rotated".to_string(), (10.0, 10.0, 20.0, 30.0)));
        }

        #[test]
        fn discards_a_span_with_a_bbox_degenerate_on_x_before_any_rotation() {
            let page = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((1.0, 0.0), (10.0, 10.0, 10.0, 30.0), vec![span("Arial", 10.0, "flat", (10.0, 10.0, 10.0, 30.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&page, false).is_empty());
        }

        #[test]
        fn discards_a_span_with_a_bbox_degenerate_on_y_before_any_rotation() {
            let page = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((1.0, 0.0), (10.0, 10.0, 30.0, 10.0), vec![span("Arial", 10.0, "flat", (10.0, 10.0, 30.0, 10.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&page, false).is_empty());
        }

        /// Without the guard this case would panic inside [`PdfLine::new`] instead of dropping the
        /// span — which a malformed page dict, reachable from untrusted input, must never be able
        /// to do.
        #[test]
        fn discards_a_span_with_a_non_positive_font_size_instead_of_panicking() {
            let page = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((1.0, 0.0), (10.0, 10.0, 20.0, 20.0), vec![span("Arial", 0.0, "zero", (10.0, 10.0, 20.0, 20.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&page, false).is_empty());

            let negative = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((1.0, 0.0), (10.0, 10.0, 20.0, 20.0), vec![span("Arial", -5.0, "negative", (10.0, 10.0, 20.0, 20.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&negative, false).is_empty());
        }

        /// Without the guard this case would panic inside [`Rectangle::new`] on the inverted box
        /// instead of dropping the span.
        #[test]
        fn discards_a_span_with_an_inverted_bbox_instead_of_panicking() {
            let page = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((1.0, 0.0), (30.0, 10.0, 10.0, 20.0), vec![span("Arial", 10.0, "inverted-x", (30.0, 10.0, 10.0, 20.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&page, false).is_empty());

            let inverted_y = PageDict {
                width: 100.0,
                height: 100.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((1.0, 0.0), (10.0, 30.0, 20.0, 10.0), vec![span("Arial", 10.0, "inverted-y", (10.0, 30.0, 20.0, 10.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&inverted_y, false).is_empty());
        }

        /// The starting line is **not** degenerate along both axes: it is degenerate only along x,
        /// an axis a quarter turn swaps with y. The test shows the discard looks at the box
        /// **after** rotation, not at the original one.
        #[test]
        fn discards_a_line_whose_rotated_bbox_is_degenerate_even_if_the_pre_rotation_bbox_was_degenerate_on_the_other_axis() {
            let page = PageDict {
                width: 100.0,
                height: 200.0,
                blocks: vec![PageDictBlock::Text {
                    lines: vec![line((0.0, 1.0), (10.0, 10.0, 10.0, 30.0), vec![span("Arial", 10.0, "flat", (10.0, 10.0, 10.0, 30.0))])],
                }],
            };
            assert!(pdflines_from_pagedict(&page, true).is_empty());
        }
    }

    mod pdfimages_from_pagedict_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn only_imageraster_blocks_produce_images() {
            let page = PageDict {
                width: 10.0,
                height: 10.0,
                blocks: vec![
                    PageDictBlock::Text { lines: vec![] },
                    PageDictBlock::ImageRaster { bbox: (1.0, 2.0, 3.0, 4.0), ext: "png".to_string(), data: vec![9, 8, 7] },
                    PageDictBlock::Other,
                ],
            };
            let images = pdfimages_from_pagedict(&page);
            assert_eq!(images.len(), 1);
            assert_eq!(images[0].bbox, Rectangle::new(1.0, 2.0, 3.0, 4.0));
            assert_eq!(images[0].ext, "png");
            assert_eq!(images[0].data, vec![9, 8, 7]);
        }

        #[test]
        fn order_is_preserved_across_multiple_images() {
            let page = PageDict {
                width: 10.0,
                height: 10.0,
                blocks: vec![
                    PageDictBlock::ImageRaster { bbox: (0.0, 0.0, 1.0, 1.0), ext: "png".to_string(), data: vec![1] },
                    PageDictBlock::ImageRaster { bbox: (2.0, 2.0, 3.0, 3.0), ext: "jpeg".to_string(), data: vec![2] },
                ],
            };
            let images = pdfimages_from_pagedict(&page);
            assert_eq!(images.len(), 2);
            assert_eq!(images[0].data, vec![1]);
            assert_eq!(images[1].data, vec![2]);
        }

        #[test]
        fn bbox_ext_and_data_are_passed_through_unchanged() {
            let page = PageDict {
                width: 10.0,
                height: 10.0,
                blocks: vec![PageDictBlock::ImageRaster { bbox: (5.0, 6.0, 7.0, 8.0), ext: "jpeg".to_string(), data: vec![1, 2, 3, 4] }],
            };
            let images = pdfimages_from_pagedict(&page);
            assert_eq!(images[0], PageImage { bbox: Rectangle::new(5.0, 6.0, 7.0, 8.0), ext: "jpeg".to_string(), data: vec![1, 2, 3, 4] });
        }

        #[test]
        fn no_image_blocks_yields_an_empty_vec() {
            let page = PageDict { width: 10.0, height: 10.0, blocks: vec![PageDictBlock::Text { lines: vec![] }, PageDictBlock::Other] };
            assert!(pdfimages_from_pagedict(&page).is_empty());
        }
    }
}
