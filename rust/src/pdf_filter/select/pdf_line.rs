pub mod font;
pub mod text;
pub mod area;
pub mod font_size;

use font::{FontSet,Font};
use text::TextSet;
use area::Area;
use font_size::FontSizeInterval;
use crate::commons::geometry::{Rectangle};
use crate::commons::sets::{Container,Overlappable,AstSet,SetRelation};

pub struct AstLeafPdfLineSet{
    font: Option<FontSet>,
    font_size: Option<FontSizeInterval>,
    text: Option<TextSet>,
    area: Option<Area>
}

impl Container for AstLeafPdfLineSet {
    type Elem = PdfLine;
    fn contains(&self, ele: &PdfLine) -> bool {
        let PdfLine{
            font,
            font_size,
            text,
            area
        } = ele;
        if self.font_size.as_ref().is_some_and(|fs| !fs.contains(font_size) ) {
            return false
        }
        if self.area.as_ref().is_some_and(|a| {
            let rel = a.set_relation(area);
            rel == SetRelation::Superset || rel == SetRelation::Equal
        }) {
            return false
        }
        if self.text.as_ref().is_some_and(|t| !t.contains(text)) {
            return false
        }
        if self.font.as_ref().is_some_and(|f| !f.contains(font)) {
            return false
        }
        true
    }
}

pub type PdfLineSet = AstSet<AstLeafPdfLineSet,PdfLine>;


pub struct PdfLine {
    font: Font,
    font_size: f32,
    text: String,
    area: Area,
}

// impl PdfLine {
//     fn new(font: Font, font_size) {

//     }
// }