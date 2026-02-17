pub mod font;
pub mod text;
pub mod area;
pub mod font_size;

use font::{FontSet,Font};
use text::TextSet;
use area::Area;
use font_size::FontSizeInterval;
use crate::commons::geometry::{Rectangle};
use crate::commons::sets::{Container,Overlappable,AstSet,SetRelation,AstNode,SmartAstNode};
use std::ops::{BitAnd,BitOr,Div};

// #[derive(Debug)]
// pub struct AstLeafPdfLineSet{
//     font: FontSet,
//     font_size: FontSizeInterval,
//     text: TextSet,
//     area: Area
// }

#[derive(Debug)]
pub enum SelectPdfLineSet {
    Font(FontSet),
    FontSize(FontSizeInterval),
    Text(TextSet),
    Area(Area)
}

// impl AstLeafPdfLineSet {
//     fn new(font: &str,font_size: (f32,f32),text: &str,area: (f32,f32,f32,f32)) -> Self {
//         Self{
//             font: FontSet::new(font),
//             font_size: FontSizeInterval::new(font_size.0,font_size.1),
//             text: TextSet::new(text),
//             area: Area::new(area.0,area.1,area.2,area.3)
//         }
//     }
//     fn from_sets(font: FontSet,font_size: FontSizeInterval,text: TextSet,area: Area) -> Self {
//         Self{
//             font,
//             font_size,
//             text,
//             area
//         }
//     }
// }

impl Container for SelectPdfLineSet {
    type Elem = PdfLine;
    fn contains(&self, ele: &PdfLine) -> bool {
        match self {
            Self::Font(a) => a.contains(&ele.font),
            Self::FontSize(a) => a.contains(&ele.font_size),
            Self::Text(a) => a.contains(&ele.text),
            Self::Area(a) => {
                let r = a.set_relation(&ele.area);
                r == SetRelation::Equal || r == SetRelation::Superset
            }
        }
    }
}

impl SelectPdfLineSet {
    fn select_font(font: &str) -> Self {
        Self::Font(FontSet::new(font))
    }
    fn select_fontsize(a: f32, b: f32) -> Self {
        Self::FontSize(FontSizeInterval::new(a,b))
    }
    fn select_text(text: &str) -> Self {
        Self::Text(TextSet::new(text))
    }
    fn select_area(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::Area(Area::new(x0,y0,x1,y1))
    }
    fn font(font: FontSet) -> Self {
        Self::Font(font)
    }
    fn fontsize(font_size: FontSizeInterval) -> Self {
        Self::FontSize(font_size)
    }
    fn text(text: TextSet) -> Self {
        Self::Text(text)
    }
    fn area(area: Area) -> Self {
        Self::Area(area)
    }
}

pub type PdfLineSet = AstSet<SelectPdfLineSet,PdfLine>;

impl PdfLineSet {
    pub fn select_font(font: &str) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::select_font(font)
        )
    }
    pub fn select_fontsize(a: f32, b: f32) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::FontSize(FontSizeInterval::new(a,b))
        )
    }
    pub fn select_text(text: &str) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::Text(TextSet::new(text))
        )
    }
    pub fn select_area(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::Area(Area::new(x0,y0,x1,y1))
        )
    }
    pub fn font(font: FontSet) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::Font(font)
        )
    }
    pub fn fontsize(font_size: FontSizeInterval) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::FontSize(font_size)
        )
    }
    pub fn text(text: TextSet) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::Text(text)
        )
    }
    pub fn area(area: Area) -> Self {
        Self::from_leaf(
            SelectPdfLineSet::Area(area)
        )
    }
    pub fn new(font: Option<&str>, font_size: Option<(f32,f32)>, text: Option<&str>, area: Option<(f32,f32,f32,f32)>) -> Self{
        match (font,font_size,text,area) {
            (None,None,None,None) => Self::select_fontsize(0.0,1e6),
            (Some(f),None,None,None) => Self::select_font(f),
            (None,Some((a,b)),None,None) => Self::select_fontsize(a,b),
            (None,None,Some(t),None) => Self::select_text(t),
            (None,None,None,Some((x0,y0,x1,y1))) => Self::select_area(x0,y0,x1,y1),
            (Some(f),Some((a,b)),None,None) => Self::select_font(f) & Self::select_fontsize(a,b),
            (Some(f),None,Some(t),None) => Self::select_font(f) & Self::select_text(t),
            (Some(f),None,None,Some((x0,y0,x1,y1))) => Self::select_font(f) & Self::select_area(x0,y0,x1,y1),
            (None,Some((a,b)),Some(t),None) => Self::select_fontsize(a,b) & Self::select_text(t),
            (None,Some((a,b)),None,Some((x0,y0,x1,y1))) => Self::select_fontsize(a,b) & Self::select_area(x0,y0,x1,y1),
            (None,None,Some(t),Some((x0,y0,x1,y1))) => Self::select_text(t) & Self::select_area(x0,y0,x1,y1),
            (Some(f),Some((a,b)),Some(t),None) => Self::select_font(f) & Self::select_fontsize(a,b) & Self::select_text(t),
            (Some(f),Some((a,b)),None,Some((x0,y0,x1,y1))) => Self::select_font(f) & Self::select_fontsize(a,b) & Self::select_area(x0,y0,x1,y1),
            (Some(f),None,Some(t),Some((x0,y0,x1,y1))) => Self::select_font(f) & Self::select_text(t) & Self::select_area(x0,y0,x1,y1),
            (None,Some((a,b)),Some(t),Some((x0,y0,x1,y1))) => Self::select_fontsize(a,b) & Self::select_text(t) & Self::select_area(x0,y0,x1,y1),
            (Some(f),Some((a,b)),Some(t),Some((x0,y0,x1,y1))) => {
                Self::select_font(f) & Self::select_fontsize(a,b) & Self::select_text(t) & Self::select_area(x0,y0,x1,y1)
            }
        }
    }
    pub fn from_sets(font: Option<FontSet>, font_size: Option<FontSizeInterval>, text: Option<TextSet>, area: Option<Area>) -> Self {
        match (font,font_size,text,area) {
            (None,None,None,None) => Self::fontsize(FontSizeInterval::new(0.0,1e6)),
            (Some(f),None,None,None) => Self::font(f),
            (None,Some(fs),None,None) => Self::fontsize(fs),
            (None,None,Some(t),None) => Self::text(t),
            (None,None,None,Some(a)) => Self::area(a),
            (Some(f),Some(fs),None,None) => Self::font(f) & Self::fontsize(fs),
            (Some(f),None,Some(t),None) => Self::font(f) & Self::text(t),
            (Some(f),None,None,Some(a)) => Self::font(f) & Self::area(a),
            (None,Some(fs),Some(t),None) => Self::fontsize(fs) & Self::text(t),
            (None,Some(fs),None,Some(a)) => Self::fontsize(fs) & Self::area(a),
            (None,None,Some(t),Some(a)) => Self::text(t) & Self::area(a),
            (Some(f),Some(fs),Some(t),None) => Self::font(f) & Self::fontsize(fs) & Self::text(t),
            (Some(f),Some(fs),None,Some(a)) => Self::font(f) & Self::fontsize(fs) & Self::area(a),
            (Some(f),None,Some(t),Some(a)) => Self::font(f) & Self::text(t) & Self::area(a),
            (None,Some(fs),Some(t),Some(a)) => Self::fontsize(fs) & Self::text(t) & Self::area(a),
            (Some(f),Some(fs),Some(t),Some(a)) => Self::font(f) & Self::fontsize(fs) & Self::text(t) & Self::area(a)
        }
    }
}

#[derive(Debug)]
pub struct PdfLine {
    font: Font,
    font_size: f32,
    text: String,
    area: Area,
    bbox: Rectangle
}

impl PdfLine {
    pub fn new(font: &str, font_size: f32, text: &str, area: (f32,f32,f32,f32)) -> Self {
        if font_size <= 0.0 {
            panic!("Font size of a PdfLine cannot be negative")
        }
        Self{
            font: Font::new(font),
            font_size: font_size,
            text: text.to_string(),
            bbox: Rectangle::new(
                area.0,
                area.1,
                area.2,
                area.3
            ),
            area: Area::from_atom(Rectangle::new(
                area.0,
                area.1,
                area.2,
                area.3
            ))
        }
    }
    pub fn font(&self) -> &Font {
        &self.font
    }
    pub fn font_size(&self) -> &f32 {
        &self.font_size
    }
    pub fn bbox(&self) -> &Rectangle {
        &self.bbox
    }
    pub fn text(&self) -> &String {
        &self.text
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    #[test]
    fn new_pdfline() {
        let font = "Arial";
        let font_size = 45.3;
        let text = "La grange muraja axur!";
        let (x0,y0,x1,y1) = (6.0,4.0,70.0,60.0);
        let line = PdfLine::new(font,font_size,text,(x0,y0,x1,y1));
        let PdfLine{
            font: res_font,
            font_size: res_font_size,
            text: res_text,
            area: res_area,
            bbox: res_bbox
        } = line;
        assert_eq!(res_font,Font::new(font));
        assert_eq!(res_font_size,font_size);
        assert_eq!(res_text,text);
        assert_eq!(res_area.atoms().iter().next().unwrap().as_tuple(),(x0,y0,x1,y1));
        assert_eq!(res_bbox.as_tuple(),(x0,y0,x1,y1));
    }

    // #[test_case(None,None,None,None;"universe")]
    // #[test_case(Some("Arial"),None,None,None;"font")]
    // #[test_case(None,Some((45.3,60.0)),None,None;"font_size")]
    // #[test_case(None,None,Some("^calipso]"),None;"text")]
    // #[test_case(None,None,None,Some((2.0,3.0,4.0,6.0));"area")]
    // #[test_case(Some("Fraktur-Black Bold"),Some((0.1,4.0)),Some("tubis$"),Some((0.0,0.0,10.0,20.0));"all specified")]
    // fn new_pdflineset(font: Option<&str>, font_size: Option<(f32,f32)>, text: Option<&str>, area: Option<(f32,f32,f32,f32)>) {
    //     let set = PdfLineSet::new(font,font_size,text,area);

    // }

    // #[test_case(None,None,None,None;"no set")]
    // #[test_case(Some(FontSet::new("Arial")),None,None,None;"fontset")]
    // #[test_case(None,Some(FontSizeInterval::new(45.3,60.0)),None,None;"font_size interval")]
    // #[test_case(None,None,Some(TextSet::new("^calipso]")),None;"textset")]
    // #[test_case(None,None,None,Some(Area::new(2.0,3.0,4.0,6.0));"area")]
    // #[test_case(
    //     Some(FontSet::new("Fraktur-Black Bold")),
    //     Some(FontSizeInterval::new(0.1,4.0)),
    //     Some(TextSet::new("tubis$")),
    //     Some(Area::new(0.0,0.0,10.0,20.0))
    // ;"all specified")]
    // fn pdflineset_from_sets(font: Option<FontSet>, font_size: Option<FontSizeInterval>, text: Option<TextSet>, area: Option<Area>) {
    //     let set = PdfLineSet::from_sets(font.clone(),font_size.clone(),text.clone(),area.clone());
    // }

    #[test]
    #[should_panic(expected="Font size of a PdfLine cannot be negative")]
    fn new_pdfline_negative_fontsize() {
        let font = "Arial";
        let font_size = -45.3;
        let text = "La grange muraja axur!";
        let (x0,y0,x1,y1) = (6.0,4.0,70.0,60.0);
        let line = PdfLine::new(font,font_size,text,(x0,y0,x1,y1));
    }

    #[test_case(
        SelectPdfLineSet::select_font("ARIAL"),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"font"
    )]
    #[test_case(
        SelectPdfLineSet::select_fontsize(0.0,100.0),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"font size"
    )]
    #[test_case(
        SelectPdfLineSet::select_text("mi$"),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"text"
    )]
    #[test_case(
        SelectPdfLineSet::select_area(0.0,0.0,2.0,2.0),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"area"
    )]
    fn element_in_leaf(set: SelectPdfLineSet, ele: PdfLine) {
        assert!(set.contains(&ele))
    }


    #[test_case(
        SelectPdfLineSet::select_font("fraktur sans-serif"),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"font"
    )]
    #[test_case(
        SelectPdfLineSet::select_fontsize(30.0,40.0),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"font size"
    )]
    #[test_case(
        SelectPdfLineSet::select_text("^rum$"),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"text"
    )]
    #[test_case(
        SelectPdfLineSet::select_area(0.1,0.0,2.0,2.0),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"area"
    )]
    fn element_not_in_leaf(set: SelectPdfLineSet, ele: PdfLine) {
        assert!(!set.contains(&ele))
    }




}