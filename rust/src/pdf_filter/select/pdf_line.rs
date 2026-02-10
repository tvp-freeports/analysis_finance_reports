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

// impl Overlappable<Self> for AstLeafPdfLineSet {
//     fn set_relation(&self, other: &Self) -> SetRelation {
//         use SetRelation::*;
//         let mut equal = 0;
//         let mut superset = 0;
//         let mut subset = 0;
//         let mut overlapping = 0;
//         let mut disjoint = 0;
//         match (self.font_size.as_ref(),other.font_size.as_ref()) {
//             (Some(a),Some(b)) => match a.set_relation(b) {
//                 Equal => equal+=1,
//                 Superset => superset+=1,
//                 Subset => subset+=1,
//                 Disjoint => disjoint+=1,
//                 Overlapping => overlapping+=1
//             },
//             (None,Some(_)) => superset+=1,
//             (Some(_),None) => subset+=1,
//             (None,None) => equal+=1
//         };
//         match (self.area.as_ref(),other.area.as_ref()) {
//             (Some(a),Some(b)) => match a.set_relation(b) {
//                 Equal => equal+=1,
//                 Superset => superset+=1,
//                 Subset => subset+=1,
//                 Disjoint => disjoint+=1,
//                 Overlapping => overlapping+=1
//             },
//             (None,Some(_)) => superset+=1,
//             (Some(_),None) => subset+=1,
//             (None,None) => equal+=1
//         };
//         match (self.text.as_ref(),other.text.as_ref()) {
//             (Some(a),Some(b)) => match a.set_relation(b) {
//                 Equal => equal+=1,
//                 Superset => superset+=1,
//                 Subset => subset+=1,
//                 Disjoint => disjoint+=1,
//                 Overlapping => overlapping+=1
//             },
//             (None,Some(_)) => superset+=1,
//             (Some(_),None) => subset+=1,
//             (None,None) => equal+=1
//         };
//         match (self.font.as_ref(),other.font.as_ref()) {
//             (Some(a),Some(b)) => match a.set_relation(b) {
//                 Equal => equal+=1,
//                 Superset => superset+=1,
//                 Subset => subset+=1,
//                 Disjoint => disjoint+=1,
//                 Overlapping => overlapping+=1
//             },
//             (None,Some(_)) => superset+=1,
//             (Some(_),None) => subset+=1,
//             (None,None) => equal+=1
//         };
        
//         if disjoint == 4 {
//             Disjoint
//         } else if equal == 4 {
//             Equal
//         } else if disjoint==0 && overlapping==0 {
//             if subset>0 && superset==0{
//                 Subset
//             } else if superset>0 && subset==0 {
//                 Superset
//             } else {
//                 Overlapping
//             }
//         } else {
//             Overlapping
//         }
//     }
// }

pub type PdfLineSet = AstSet<AstLeafPdfLineSet,PdfLine>;


pub struct PdfLine {
    font: Font,
    font_size: f32,
    text: String,
    area: Area,
}