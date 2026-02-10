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

#[derive(Debug)]
pub struct AstLeafPdfLineSet{
    font: Option<FontSet>,
    font_size: Option<FontSizeInterval>,
    text: Option<TextSet>,
    area: Option<Area>
}

impl AstLeafPdfLineSet {
    fn new(font: Option<&str>,font_size: Option<(f32,f32)>,text: Option<&str>,area: Option<(f32,f32,f32,f32)>) -> Self {
        Self{
            font: font.map(|f| FontSet::new(f)),
            font_size: font_size.map(|(a,b)| FontSizeInterval::new(a,b)),
            text: text.map(|t| TextSet::new(t)),
            area: area.map(|(x0,y0,x1,y1)| Area::new(x0,y0,x1,y1))
        }
    }
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
            rel != SetRelation::Superset && rel != SetRelation::Equal
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

impl PdfLineSet {
    fn new(font: Option<&str>,font_size: Option<(f32,f32)>,text: Option<&str>,area: Option<(f32,f32,f32,f32)>) -> Self {
        Self::from_leaf(
            AstLeafPdfLineSet::new(font,font_size,text,area)
        )
    }
}



#[derive(Debug)]
pub struct PdfLine {
    font: Font,
    font_size: f32,
    text: String,
    area: Area,
}

impl PdfLine {
    fn new(font: &str, font_size: f32, text: &str, area: (f32,f32,f32,f32)) -> Self {
        if font_size <= 0.0 {
            panic!("Font size of a PdfLine cannot be negative")
        }
        Self{
            font: Font::new(font),
            font_size: font_size,
            text: text.to_string(),
            area: Area::from_atom(Rectangle::new(
                area.0,
                area.1,
                area.2,
                area.3
            ))
        }
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
            area: res_area
        } = line;
        assert_eq!(res_font,Font::new(font));
        assert_eq!(res_font_size,font_size);
        assert_eq!(res_text,text);
        assert_eq!(res_area.atoms().iter().next().unwrap().as_tuple(),(x0,y0,x1,y1));
    }

    #[test]
    #[should_panic(expected="Font size of a PdfLine cannot be negative")]
    fn new_pdfline_negative_fontsize() {
        let font = "Arial";
        let font_size = -45.3;
        let text = "La grange muraja axur!";
        let (x0,y0,x1,y1) = (6.0,4.0,70.0,60.0);
        let line = PdfLine::new(font,font_size,text,(x0,y0,x1,y1));
        let PdfLine{
            font: res_font,
            font_size: res_font_size,
            text: res_text,
            area: res_area
        } = line;
        assert_eq!(res_font,Font::new(font));
        assert_eq!(res_font_size,font_size);
        assert_eq!(res_text,text);
        assert_eq!(res_area.atoms().iter().next().unwrap().as_tuple(),(x0,y0,x1,y1));
    }
    #[test_case(None,None,None,None;"universe")]
    #[test_case(Some("Arial"),None,None,None;"font")]
    #[test_case(None,Some((45.3,60.0)),None,None;"font_size")]
    #[test_case(None,None,Some("^calipso]"),None;"text")]
    #[test_case(None,None,None,Some((2.0,3.0,4.0,6.0));"area")]
    #[test_case(Some("Fraktur-Black Bold"),Some((0.1,4.0)),Some("tubis$"),Some((0.0,0.0,10.0,20.0));"all specified")]
    fn new_pdflineset(font: Option<&str>, font_size: Option<(f32,f32)>, text: Option<&str>, area: Option<(f32,f32,f32,f32)>) {
        let set = PdfLineSet::new(font,font_size,text,area);
        match set.ast() {
            AstNode::Leaf(AstLeafPdfLineSet{
                font: res_font,
                font_size: res_font_size,
                text: res_text,
                area: res_area
            }) => {
                match (res_font,font) {
                    (Some(f),Some(exp_f)) => assert_eq!(f.atoms().iter().next().unwrap(),&Font::new(exp_f)),
                    (None,None) => (),
                    _ => panic!("Unexpected result structure")
                };
                match (res_font_size,font_size) {
                    (Some(fs),Some(exp_fs)) => assert_eq!(fs.atoms().iter().next().unwrap().as_tuple(),exp_fs),
                    (None,None) => (),
                    _ => panic!("Unexpected result structure")
                }
                match (res_text,text) {
                    (Some(t),Some(exp_t)) => {
                        let et = TextSet::new(exp_t);
                        match (t.ast(),et.ast()) {
                            (SmartAstNode::Leaf(lt),SmartAstNode::Leaf(elt)) => assert_eq!(lt,elt),
                            _ => panic!("Unexpected result structure")
                        }
                    },
                    (None,None) => (),
                    _ => panic!("Unexpected result structure")
                }
                match (res_area,area) {
                    (Some(a),Some(exp_a)) => assert_eq!(a.atoms().iter().next().unwrap().as_tuple(),exp_a),
                    (None,None) => (),
                    _ => panic!("Unexpected result structure")
                }
                
            },
            _ => panic!("Expected a Leaf Node found a branch")
        };
    }

    #[test_case(
        AstLeafPdfLineSet::new(None,None,None,None),
        PdfLine::new("Arial",43.2,"rumi",(0.0,0.0,1.0,1.0));"universe"
    )]
    #[test_case(
        AstLeafPdfLineSet::new(Some("\tarial "),None,None,None),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"font"
    )]
    #[test_case(
        AstLeafPdfLineSet::new(None,Some((30.0,50.0)),None,None),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"font size"
    )]
    #[test_case(
        AstLeafPdfLineSet::new(None,None,Some("^rum"),None),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"text"
    )]
    #[test_case(
        AstLeafPdfLineSet::new(None,None,None,Some((0.0,0.0,2.0,2.0))),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"area"
    )]
    #[test_case(
        AstLeafPdfLineSet::new(Some("ARIAL"),Some((0.0,100.0)),Some("mi$"),Some((0.0,0.0,2.0,2.0))),
        PdfLine::new("Arial\n",43.2,"rumi",(0.0,0.0,1.0,1.0));"all specified"
    )]
    fn element_in_leaf(set: AstLeafPdfLineSet, ele: PdfLine) {
        assert!(set.contains(&ele))
    }

}