use std::cmp::max;
use ordered_float::OrderedFloat;
use crate::commons::sets::{
    Container,
    SetRelation,
    AstNode
};
use super::pdf_line::{
    area::{Area},
    font::{FontSet},
    font_size::{FontSizeInterval},
    text::{TextSet}
};

use super::pdf_line::{PdfLineSet,PdfLine};

trait RelativeInfo<V> {
    fn contextualize(self,lines: &[PdfLine]) -> V;
}

enum OptionallyRelative<V,R> {
    Absolute(V),
    Relative(R)
}

type OptRel<V,R> = OptionallyRelative<V,R>;

impl<V,R> RelativeInfo<V> for OptionallyRelative<V,R> 
where
    R: RelativeInfo<V>
{
    fn contextualize(self,lines: &[PdfLine]) -> V {
        match self {
            Self::Absolute(a) => a,
            Self::Relative(ra) => ra.contextualize(lines)
        }
    }
}

struct RelativePdfLineSet {
    font: OptRel<Option<FontSet>,RelativeFontSet>,
    font_size: OptRel<Option<FontSizeInterval>,RelativeFontSizeInterval>,
    text: OptRel<Option<TextSet>,RelativeTextSet>,
    area: OptRel<Option<Area>,RelativeArea>

}

impl RelativeInfo<PdfLineSet> for RelativePdfLineSet {
    fn contextualize(self,lines: &[PdfLine]) -> PdfLineSet {
        let font = self.font.contextualize(lines);
        let font_size = self.font_size.contextualize(lines);
        let text = self.text.contextualize(lines);
        let area = self.area.contextualize(lines);
        PdfLineSet::from_sets(font,font_size,text,area)
    }
}



enum RelativeArea {
    Select(Box<RelativePdfLineSet>),
    MoveWindow{
        target: Box<RelativePdfLineSet>,
        vec: (f32,f32),
        width_mult: f32,
        height_mult: f32
    },
    Bounds{
        x0: OptRel<f32,Box<RelativePdfLineSet>>,
        y0: OptRel<f32,Box<RelativePdfLineSet>>,
        x1: OptRel<f32,Box<RelativePdfLineSet>>,
        y1: OptRel<f32,Box<RelativePdfLineSet>>
    }

}

enum RelativeFontSet {
    Select(Box<RelativePdfLineSet>)
}

enum RelativeFontSizeInterval {
    Select(Box<RelativePdfLineSet>)
}

enum RelativeTextSet {
    Select(Box<RelativePdfLineSet>)
}


impl RelativeInfo<Option<FontSet>> for RelativeFontSet {
    fn contextualize(self,lines: &[PdfLine]) -> Option<FontSet> {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| FontSet::from_atom(l.font().clone()))
        .reduce(|a,b| a | b)
    }
}

impl RelativeInfo<Option<TextSet>> for RelativeTextSet {
    fn contextualize(self,lines: &[PdfLine]) -> Option<TextSet> {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| TextSet::new(&format!("^{}$",l.text())))
        .reduce(|a,b| a | b)
    }
}

impl RelativeInfo<Option<FontSizeInterval>> for RelativeFontSizeInterval {
    fn contextualize(self,lines: &[PdfLine]) -> Option<FontSizeInterval> {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| {
            let fs = *l.font_size();
            let a = max(OrderedFloat(0.0),OrderedFloat(fs-1e-4)).into_inner();
            FontSizeInterval::new(a,fs+1e-4)
        })
        .reduce(|a,b| a | b)
    }
}


impl RelativeArea {
    fn from_movewindow(
        lines: &[PdfLine],
        target: RelativePdfLineSet,
        vec: (f32,f32),
        width_mult: f32,
        height_mult: f32
    ) -> Option<Area> {
        let line_set = target.contextualize(lines);
        let (x,y) = vec;
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| l.bbox().as_tuple())
        .next().map(|(x0,y0,x1,y1)| {
            let w = x1-x0;
            let h = y1-y0;
            Area::new(x0+x*w,y0+y*h,x0+(width_mult+x)*w,y0+(height_mult+y)*h)
        })
    }
    fn from_bounds(
        lines: &[PdfLine],
        x0: OptRel<f32,Box<RelativePdfLineSet>>,
        y0: OptRel<f32,Box<RelativePdfLineSet>>,
        x1: OptRel<f32,Box<RelativePdfLineSet>>,
        y1: OptRel<f32,Box<RelativePdfLineSet>>
    ) -> Option<Area> {
        let left=match x0 {
            OptionallyRelative::Absolute(x) => x,
            OptionallyRelative::Relative(rls) => {
                let line_set=rls.contextualize(lines);
                match lines.iter()
                .filter(|l| line_set.contains(l))
                .map(|l| l.bbox().as_tuple())
                .next() {
                    Some((_,_,x1,_)) => x1,
                    None => 0.0
                }
            }
        };
        let right=match x1 {
            OptionallyRelative::Absolute(x) => x,
            OptionallyRelative::Relative(rls) => {
                let line_set=rls.contextualize(lines);
                match lines.iter()
                .filter(|l| line_set.contains(l))
                .map(|l| l.bbox().as_tuple())
                .next() {
                    Some((x0,_,_,_)) => x0,
                    None => 10e+6
                }
            }
        };
        let up=match y0 {
            OptionallyRelative::Absolute(y) => y,
            OptionallyRelative::Relative(rls) => {
                let line_set=rls.contextualize(lines);
                match lines.iter()
                .filter(|l| line_set.contains(l))
                .map(|l| l.bbox().as_tuple())
                .next() {
                    Some((_,_,_,y1)) => y1,
                    None => 0.0
                }
            }
        };
        let bottom=match y1 {
            OptionallyRelative::Absolute(y) => y,
            OptionallyRelative::Relative(rls) => {
                let line_set=rls.contextualize(lines);
                match lines.iter()
                .filter(|l| line_set.contains(l))
                .map(|l| l.bbox().as_tuple())
                .next() {
                    Some((_,y0,_,_)) => y0,
                    None => 10e+6
                }
            }
        };
        Some(Area::new(left,up,right,bottom))
    }
    fn from_selection(lines: &[PdfLine], set: RelativePdfLineSet) -> Option<Area> {
        let line_set = set.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| Area::from_atom(*l.bbox()))
        .reduce(|a,b| a | b)
    }
}

impl RelativeInfo<Option<Area>> for RelativeArea {
    fn contextualize(self,lines: &[PdfLine]) -> Option<Area> {
        match self {
            Self::Select(r) => Self::from_selection(lines,*r),
            Self::MoveWindow{
                target,
                vec,
                width_mult,
                height_mult
            } => Self::from_movewindow(lines,*target,vec,width_mult,height_mult),
            Self::Bounds{x0,y0,x1,y1} => Self::from_bounds(lines,x0,y0,x1,y1)
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use std::sync::LazyLock;
    use OptionallyRelative::*;
    static LINES: LazyLock<Vec<PdfLine>> = LazyLock::new(|| vec![
        PdfLine::new("Arial",45.0,"TITLE OF THE PAGE",(35.0,1.0,65.0,5.0)),
        PdfLine::new("A",1.5,"text",(10.0,10.0,15.0,11.0)),
        PdfLine::new("A",1.7,"with",(10.0,11.0,15.0,12.0)),
        PdfLine::new("C",1.3,"similar",(10.0,12.0,15.0,13.0)),
        PdfLine::new("D",1.1,"font",(10.0,13.0,15.0,14.0)),
        PdfLine::new("E",1.13,"size",(10.0,14.0,15.0,15.0)),
        PdfLine::new("Fracktur",40.0,"SECTION 2",(35.0,21.0,65.0,25.0)),
        PdfLine::new("A",14.5,"same",(10.0,30.0,15.0,31.0)),
        PdfLine::new("A",188.7,"font",(10.0,31.0,15.0,32.0)),
        PdfLine::new("B",0.3,"-----",(10.0,32.0,15.0,33.0)),
        PdfLine::new("DDD",14.1,"font",(10.0,33.0,15.0,34.0)),
        PdfLine::new("EEE",14.13,"size",(10.0,34.0,15.0,35.0))
    ]);
    
    #[test_case(RelativePdfLineSet{
        font: Absolute(None),
        font_size: Absolute(None),
        text: Absolute(None),
        area: Absolute(None)
    },PdfLineSet::new(None,None,None,None);"universe")]
    #[test_case(RelativePdfLineSet{
        font: Absolute(Some(FontSet::new("Gamino"))),
        font_size: Absolute(None),
        text: Absolute(None),
        area: Absolute(None)
    },PdfLineSet::new(Some("Gamino"),None,None,None);"font")]
    #[test_case(RelativePdfLineSet{
        font: Absolute(None),
        font_size: Absolute(Some(FontSizeInterval::new(0.1,20.0))),
        text: Absolute(None),
        area: Absolute(None)
    },PdfLineSet::new(None,Some((0.1,20.0)),None,None);"font size")]
    #[test_case(RelativePdfLineSet{
        font: Absolute(None),
        font_size: Absolute(None),
        text: Absolute(Some(TextSet::new("^hul$"))),
        area: Absolute(None)
    },PdfLineSet::new(None,None,Some("^hul$"),None);"text")]
    #[test_case(RelativePdfLineSet{
        font: Absolute(None),
        font_size: Absolute(None),
        text: Absolute(None),
        area: Absolute(Some(Area::new(0.0,0.0,40.0,45.0)))
    },PdfLineSet::new(None,None,None,Some((0.0,0.0,40.0,45.0)));"area")]
    fn contextualize_trivial_relativepdflineset(rls: RelativePdfLineSet ,exp_ls: PdfLineSet) {
        use SetRelation::*;
        let ls=rls.contextualize(&LINES);
        let (AstNode::Leaf(leaf),AstNode::Leaf(eleaf)) = (ls.ast(),exp_ls.ast()) else {
            panic!("Unexpected result structure")
        };
        match (&leaf.font,&eleaf.font) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.atoms(),b.atoms()),
            _ => panic!("Unexpected result structure")
        };
        match (&leaf.font_size,&eleaf.font_size) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.atoms(),b.atoms()),
            _ => panic!("Unexpected result structure")
        };
        match (&leaf.area,&eleaf.area) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.atoms(),b.atoms()),
            _ => panic!("Unexpected result structure")
        };
        match (&leaf.text,&eleaf.text) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.ast(),b.ast()),
            _ => panic!("Unexpected result structure")
        };
        
    }

    #[test_case(RelativeFontSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(Some(FontSet::new("A"))),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(FontSet::new("A"));"select font")]
    #[test_case(RelativeFontSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(Some(FontSizeInterval::new(1.09,1.8))),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(
        FontSet::new("A") | FontSet::new("C") | FontSet::new("D") | FontSet::new("E")
    );"select font size")]
    #[test_case(RelativeFontSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(Some(TextSet::new("fon"))),
            area: Absolute(None)
        })
    ),Some(
        FontSet::new("A") | FontSet::new("D") | FontSet::new("DDD")
    );"select text")]
    #[test_case(RelativeFontSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(0.0,30.0,20.0,34.0)))
        })
    ),Some(
        FontSet::new("A") | FontSet::new("B") | FontSet::new("DDD") 
    );"select area")]
    #[test_case(RelativeFontSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(800.0,30.0,2000.0,340.0)))
        })
    ),None;"no result")]
    fn contextualize_fontset(rf: RelativeFontSet, exp_f: Option<FontSet>) {
        let f=rf.contextualize(&LINES);
        match (&f,&exp_f) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.atoms(),b.atoms()),
            _ => panic!("Unexpected result structure")
        };
    }


    #[test_case(RelativeFontSizeInterval::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(Some(FontSet::new("A"))),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(
        FontSizeInterval::from_precision(1.5,1e-4) | FontSizeInterval::from_precision(1.7,1e-4) | FontSizeInterval::from_precision(14.5,1e-4) | FontSizeInterval::from_precision(188.7,1e-4)
    );"select font")]
    #[test_case(RelativeFontSizeInterval::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(Some(FontSizeInterval::new(1.2,1.8))),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(
        FontSizeInterval::from_precision(1.3,1e-4) | FontSizeInterval::from_precision(1.5,1e-4) | FontSizeInterval::from_precision(1.7,1e-4)
    );"select font size")]
    #[test_case(RelativeFontSizeInterval::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(Some(TextSet::new("size"))),
            area: Absolute(None)
        })
    ),Some(
        FontSizeInterval::from_precision(1.13,1e-4) | FontSizeInterval::from_precision(14.13,1e-4)
    );"select text")]
    #[test_case(RelativeFontSizeInterval::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(0.0,30.0,20.0,33.0)))
        })
    ),Some(
        FontSizeInterval::from_precision(14.5,1e-4) | FontSizeInterval::from_precision(188.7,1e-4) | FontSizeInterval::from_precision(0.3,1e-4)
    );"select area")]
    #[test_case(RelativeFontSizeInterval::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(800.0,30.0,2000.0,340.0)))
        })
    ),None;"no result")]
    fn contextualize_fontsizeinterval(rfs: RelativeFontSizeInterval, exp_fs: Option<FontSizeInterval>) {
        let fs=rfs.contextualize(&LINES);
        match (&fs,&exp_fs) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.atoms(),b.atoms()),
            _ => panic!("Unexpected result structure")
        };
    }

    #[test_case(RelativeTextSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(Some(FontSet::new("A"))),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(
        TextSet::new("^text$") | TextSet::new("^with$")  | TextSet::new("^same$") | TextSet::new("^font$")
    );"select font")]
    #[test_case(RelativeTextSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(Some(FontSizeInterval::new(1.2,1.8))),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(
        TextSet::new("^text$") | TextSet::new("^with$") | TextSet::new("^similar$")
    );"select font size")]
    #[test_case(RelativeTextSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(Some(TextSet::new("i"))),
            area: Absolute(None)
        })
    ),Some(
        TextSet::new("^with$") | TextSet::new("^similar$") | TextSet::new("^size$") | TextSet::new("^size$")
    );"select text")]
    #[test_case(RelativeTextSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(0.0,30.0,20.0,33.0)))
        })
    ),Some(
        TextSet::new("^same$") | TextSet::new("^font$") | TextSet::new("^-----$")
    );"select area")]
    #[test_case(RelativeTextSet::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(800.0,30.0,2000.0,340.0)))
        })
    ),None;"no result")]
    fn contextualize_textset(rt: RelativeTextSet, exp_t: Option<TextSet>) {
        let t=rt.contextualize(&LINES);
        match (&t,&exp_t) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.ast(),b.ast()),
            _ => panic!("Unexpected result structure")
        };
    }


    #[test_case(RelativeArea::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(Some(FontSet::new("A"))),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(None)
        })
    ),Some(
        Area::new(10.0,10.0,15.0,11.0) | Area::new(10.0,11.0,15.0,12.0) | Area::new(10.0,11.0,15.0,12.0) | Area::new(10.0,30.0,15.0,31.0) | Area::new(10.0,31.0,15.0,32.0)
    );"select font")]
    // #[test_case(RelativeArea::Select(
    //     Box::new(RelativePdfLineSet{
    //         font: Absolute(None),
    //         font_size: Absolute(Some(FontSizeInterval::new(1.2,1.8))),
    //         text: Absolute(None),
    //         area: Absolute(None)
    //     })
    // ),Some(
    //     FontSizeInterval::from_precision(1.3,1e-4) | FontSizeInterval::from_precision(1.5,1e-4) | FontSizeInterval::from_precision(1.7,1e-4)
    // );"select font size")]
    // #[test_case(RelativeArea::Select(
    //     Box::new(RelativePdfLineSet{
    //         font: Absolute(None),
    //         font_size: Absolute(None),
    //         text: Absolute(Some(TextSet::new("size"))),
    //         area: Absolute(None)
    //     })
    // ),Some(
    //     FontSizeInterval::from_precision(1.13,1e-4) | FontSizeInterval::from_precision(14.13,1e-4)
    // );"select text")]
    // #[test_case(RelativeArea::Select(
    //     Box::new(RelativePdfLineSet{
    //         font: Absolute(None),
    //         font_size: Absolute(None),
    //         text: Absolute(None),
    //         area: Absolute(Some(Area::new(0.0,30.0,20.0,33.0)))
    //     })
    // ),Some(
    //     FontSizeInterval::from_precision(14.5,1e-4) | FontSizeInterval::from_precision(188.7,1e-4) | FontSizeInterval::from_precision(0.3,1e-4)
    // );"select area")]
    #[test_case(RelativeArea::Select(
        Box::new(RelativePdfLineSet{
            font: Absolute(None),
            font_size: Absolute(None),
            text: Absolute(None),
            area: Absolute(Some(Area::new(800.0,30.0,2000.0,340.0)))
        })
    ),None;"no result")]
    fn contextualize_area(rfs: RelativeArea, exp_fs: Option<Area>) {
        let fs=rfs.contextualize(&LINES);
        match (&fs,&exp_fs) {
            (None,None) => (),
            (Some(a),Some(b)) => assert_eq!(a.atoms(),b.atoms()),
            _ => panic!("Unexpected result structure")
        };
    }


}