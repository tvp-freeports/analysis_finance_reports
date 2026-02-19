use std::cmp::max;
use std::ops::{BitOr,BitAnd,Div};
use ordered_float::OrderedFloat;
use crate::commons::sets::{SetOps,Container};
use super::pdf_line::{
    area::{Area},
    font::{FontSet},
    font_size::{FontSizeInterval},
    text::{TextSet}
};

use super::pdf_line::{SelectPdfLineSet,PdfLineSet,PdfLine};

pub trait RelativeInfo<V> {
    fn contextualize(self,lines: &[PdfLine]) -> V;
}

// #[derive(Debug)]
pub enum OptionallyRelative<V,R> {
    Absolute(V),
    Relative(R)
}

impl<V,R> Clone for OptionallyRelative<V,R> 
where
    V: Clone,
    R: Clone
{
    fn clone(&self) -> Self {
        match self {
            Self::Absolute(a) => Self::Absolute(a.clone()),
            Self::Relative(a) => Self::Relative(a.clone())
        }        
    }
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


// #[derive(Debug)]
#[derive(Clone)]
pub enum RelativeSelectPdfLineSet {
    Font(RelativeFontSet),
    FontSize(RelativeFontSizeInterval),
    Text(RelativeTextSet),
    Area(RelativeArea)
}

impl RelativeSelectPdfLineSet {
    pub fn select_font_of(target: PdfLineSelection) -> Self {
        Self::Font(RelativeFontSet::from_selection(target))
    }
    pub fn select_fontsize_of(target: PdfLineSelection) -> Self {
        Self::FontSize(RelativeFontSizeInterval::from_selection(target))
    }
    pub fn select_text_of(target: PdfLineSelection) -> Self {
        Self::Text(RelativeTextSet::from_selection(target))
    }
    pub fn select_area_of(target: PdfLineSelection) -> Self {
        Self::Area(RelativeArea::from_selection(target))
    }
    pub fn area_from_movewindow(target: PdfLineSelection,vec: (f32,f32), width_mult: f32, height_mult: f32) -> Self {
        Self::Area(RelativeArea::from_movewindow(target,vec,width_mult,height_mult))
    }
}

impl RelativeInfo<SelectPdfLineSet> for RelativeSelectPdfLineSet{
    fn contextualize(self, lines: &[PdfLine]) -> SelectPdfLineSet {
        use SelectPdfLineSet::*;
        match self {
            Self::Font(rf) => Font(rf.contextualize(lines)),
            Self::FontSize(rfs) => FontSize(rfs.contextualize(lines)),
            Self::Text(rt) => Text(rt.contextualize(lines)),
            Self::Area(ra) => Area(ra.contextualize(lines)),
        }
    }
}

type LeafType = OptRel<SelectPdfLineSet,RelativeSelectPdfLineSet>;



// #[derive(Debug)]
enum NodeRelativePdfLineSet {
    Leaf(LeafType),
    Branch(Box<NodeRelativePdfLineSet>, SetOps, Box<NodeRelativePdfLineSet>)
}

impl Clone for NodeRelativePdfLineSet {
    fn clone(&self) -> Self {
        match self {
            Self::Leaf(a) => Self::Leaf(a.clone()),
            Self::Branch(a,ops,b) => Self::Branch(a.clone(),ops.clone(),b.clone())
        }
    }
}


impl RelativeInfo<PdfLineSet> for NodeRelativePdfLineSet {
    fn contextualize(self,lines: &[PdfLine]) -> PdfLineSet {
        use SetOps::*;
        match self {
            Self::Leaf(leaf) => PdfLineSet::from_leaf(leaf.contextualize(lines)),
            Self::Branch(box_x,op,box_y) => {
                let a = box_x.contextualize(lines);
                let b = box_y.contextualize(lines);
                match op {
                    Union => a | b,
                    Inter => a & b,
                    Sub => a / b
                }
            }
        }
    }
}



// #[derive(Debug)]
pub struct RelativePdfLineSet(NodeRelativePdfLineSet);


impl Clone for RelativePdfLineSet {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}



impl RelativeInfo<PdfLineSet> for RelativePdfLineSet {
    fn contextualize(self,lines: &[PdfLine]) -> PdfLineSet {
        self.0.contextualize(lines)
    }
}

impl BitOr<Self> for RelativePdfLineSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(
            NodeRelativePdfLineSet::Branch(
                Box::new(self.0),
                SetOps::Union,
                Box::new(rhs.0)
            )
        )
    }
}
impl BitAnd<Self> for RelativePdfLineSet {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(
            NodeRelativePdfLineSet::Branch(
                Box::new(self.0),
                SetOps::Inter,
                Box::new(rhs.0)
            )
        )
    }
}
impl Div<Self> for RelativePdfLineSet {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(
            NodeRelativePdfLineSet::Branch(
                Box::new(self.0),
                SetOps::Sub,
                Box::new(rhs.0)
            )
        )
    }
}


impl RelativePdfLineSet {
    pub fn from_font(f: OptRel<FontSet,RelativeFontSet>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match f {
            Absolute(af) => Self::from_leaf(Absolute(Font(af))),
            Relative(rf) => Self::from_leaf(Relative(R::Font(rf)))
        }
    }
    pub fn from_fontsize(fs: OptRel<FontSizeInterval,RelativeFontSizeInterval>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match fs {
            Absolute(afs) => Self::from_leaf(Absolute(FontSize(afs))),
            Relative(rfs) => Self::from_leaf(Relative(R::FontSize(rfs)))
        }
    }
    pub fn from_text(t: OptRel<TextSet,RelativeTextSet>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match t {
            Absolute(at) => Self::from_leaf(Absolute(Text(at))),
            Relative(rt) => Self::from_leaf(Relative(R::Text(rt)))
        }
    }
    pub fn from_area(a: OptRel<Area,RelativeArea>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match a {
            Absolute(aa) => Self::from_leaf(Absolute(Area(aa))),
            Relative(ra) => Self::from_leaf(Relative(R::Area(ra)))
        }
    }

    pub fn from_leaf(leaf: LeafType) -> Self {
        Self(NodeRelativePdfLineSet::Leaf(leaf))
    }
    pub fn ast(&self) -> &NodeRelativePdfLineSet {
        &self.0
    }
}

pub type PdfLineSelection = OptRel<PdfLineSet,RelativePdfLineSet>;

impl PdfLineSelection {
    pub fn from_font(f: OptRel<FontSet,RelativeFontSet>) -> Self {
        use OptionallyRelative::*;
        match f {
            Absolute(af) => Absolute(PdfLineSet::font(af)),
            Relative(rf) => Relative(RelativePdfLineSet::from_font(Relative(rf)))
        }
    }
    pub fn from_fontsize(fs: OptRel<FontSizeInterval,RelativeFontSizeInterval>) -> Self {
        use OptionallyRelative::*;
        match fs {
            Absolute(afs) => Absolute(PdfLineSet::fontsize(afs)),
            Relative(rfs) => Relative(RelativePdfLineSet::from_fontsize(Relative(rfs)))
        }
    }
    pub fn from_text(t: OptRel<TextSet,RelativeTextSet>) -> Self {
        use OptionallyRelative::*;
        match t {
            Absolute(at) => Absolute(PdfLineSet::text(at)),
            Relative(rt) => Relative(RelativePdfLineSet::from_text(Relative(rt)))
        }
    }
    pub fn from_area(a: OptRel<Area,RelativeArea>) -> Self {
        use OptionallyRelative::*;
        match a {
            Absolute(aa) => Absolute(PdfLineSet::area(aa)),
            Relative(ra) => Relative(RelativePdfLineSet::from_area(Relative(ra)))
        }
    }
}

#[derive(Clone)]
enum RelativeArea {
    Select(Box<PdfLineSelection>),
    MoveWindow{
        target: Box<PdfLineSelection>,
        vec: (f32,f32),
        width_mult: f32,
        height_mult: f32
    },
    Bounds{
        x0: OptRel<f32,Box<PdfLineSelection>>,
        y0: OptRel<f32,Box<PdfLineSelection>>,
        x1: OptRel<f32,Box<PdfLineSelection>>,
        y1: OptRel<f32,Box<PdfLineSelection>>
    }

}
#[derive(Clone)]
enum RelativeFontSet {
    Select(Box<PdfLineSelection>)
}
impl RelativeFontSet {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
}

#[derive(Clone)]
enum RelativeFontSizeInterval {
    Select(Box<PdfLineSelection>)
}
impl RelativeFontSizeInterval {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
}
#[derive(Clone)]
enum RelativeTextSet {
    Select(Box<PdfLineSelection>)
}
impl RelativeTextSet {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
}

impl RelativeInfo<FontSet> for RelativeFontSet {
    fn contextualize(self,lines: &[PdfLine]) -> FontSet {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| FontSet::from_atom(l.font().clone()))
        .reduce(|a,b| a | b).unwrap_or(FontSet::empty())
    }
}

impl RelativeInfo<TextSet> for RelativeTextSet {
    fn contextualize(self,lines: &[PdfLine]) -> TextSet {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| TextSet::new(&format!("^{}$",l.text())))
        .reduce(|a,b| a | b).unwrap_or(TextSet::empty())
    }
}

impl RelativeInfo<FontSizeInterval> for RelativeFontSizeInterval {
    fn contextualize(self,lines: &[PdfLine]) -> FontSizeInterval {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| {
            let fs = *l.font_size();
            let a = max(OrderedFloat(0.0),OrderedFloat(fs-1e-4)).into_inner();
            FontSizeInterval::new(a,fs+1e-4)
        })
        .reduce(|a,b| a | b).unwrap_or(FontSizeInterval::empty())
    }
}

impl RelativeArea {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
    fn from_movewindow(target: PdfLineSelection, vec: (f32,f32), width_mult: f32, height_mult: f32) -> Self{
        Self::MoveWindow{
            target: Box::new(target),
            vec,
            width_mult,
            height_mult,
        }
    }
    fn contextualize_movewindow(
        lines: &[PdfLine],
        target: PdfLineSelection,
        vec: (f32,f32),
        width_mult: f32,
        height_mult: f32
    ) -> Area {
        let line_set = target.contextualize(lines);
        let (x,y) = vec;
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| l.bbox().as_tuple())
        .next().map(|(x0,y0,x1,y1)| {
            let w = x1-x0;
            let h = y1-y0;
            Area::new(x0+x*w,y0+y*h,x0+(width_mult+x)*w,y0+(height_mult+y)*h)
        }).unwrap_or(
            Area::empty()
        )
    }
    fn contextualize_bounds(
        lines: &[PdfLine],
        x0: OptRel<f32,Box<PdfLineSelection>>,
        y0: OptRel<f32,Box<PdfLineSelection>>,
        x1: OptRel<f32,Box<PdfLineSelection>>,
        y1: OptRel<f32,Box<PdfLineSelection>>
    ) -> Area {
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
        Area::new(left,up,right,bottom)
    }
    fn contextualize_selection(lines: &[PdfLine], set: PdfLineSelection) -> Area {
        let line_set = set.contextualize(lines);
        lines.iter()
        .filter(|l| line_set.contains(l))
        .map(|l| Area::from_atom(*l.bbox()))
        .reduce(|a,b| a | b).unwrap_or(Area::empty())
    }
}

impl RelativeInfo<Area> for RelativeArea {
    fn contextualize(self,lines: &[PdfLine]) -> Area {
        match self {
            Self::Select(r) => Self::contextualize_selection(lines,*r),
            Self::MoveWindow{
                target,
                vec,
                width_mult,
                height_mult
            } => Self::contextualize_movewindow(lines,*target,vec,width_mult,height_mult),
            Self::Bounds{x0,y0,x1,y1} => Self::contextualize_bounds(lines,x0,y0,x1,y1)
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

    #[test_case(RelativeFontSet::from_selection(
        PdfLineSelection::from_font(Absolute(FontSet::new("A")))
    ),FontSet::new("A");"select font")]
    #[test_case(RelativeFontSet::from_selection(
        PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.09,1.8)))
    ), FontSet::new("A") | FontSet::new("C") | FontSet::new("D") | FontSet::new("E")
    ;"select font size")]
    #[test_case(RelativeFontSet::from_selection(
        PdfLineSelection::from_text(Absolute(TextSet::new("fon")))
    ), FontSet::new("A") | FontSet::new("D") | FontSet::new("DDD")
    ;"select text")]
    #[test_case(RelativeFontSet::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(0.0,30.0,20.0,34.0)))
    ), FontSet::new("A") | FontSet::new("B") | FontSet::new("DDD")
    ;"select area")]
    #[test_case(RelativeFontSet::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(800.0,30.0,2000.0,340.0)))
    ), FontSet::empty();"no result")]
    fn contextualize_fontset(rf: RelativeFontSet, exp_f: FontSet) {
        let f=rf.contextualize(&LINES);
        assert_eq!(f.atoms(),exp_f.atoms());
    }

    #[test_case(RelativeFontSizeInterval::from_selection(
        PdfLineSelection::from_font(Absolute(FontSet::new("A")))
    ), FontSizeInterval::from_precision(1.5,1e-4) | FontSizeInterval::from_precision(1.7,1e-4) | FontSizeInterval::from_precision(14.5,1e-4) | FontSizeInterval::from_precision(188.7,1e-4)
    ;"select font")]
    #[test_case(RelativeFontSizeInterval::from_selection(
        PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.14,1.8)))
    ), FontSizeInterval::from_precision(1.3,1e-4) | FontSizeInterval::from_precision(1.5,1e-4) | FontSizeInterval::from_precision(1.7,1e-4)
    ;"select font size")]
    #[test_case(RelativeFontSizeInterval::from_selection(
        PdfLineSelection::from_text(Absolute(TextSet::new("size")))
    ), FontSizeInterval::from_precision(1.13,1e-4) | FontSizeInterval::from_precision(14.13,1e-4)
    ;"select text")]
    #[test_case(RelativeFontSizeInterval::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(0.0,30.0,20.0,33.0)))
    ), FontSizeInterval::from_precision(14.5,1e-4) | FontSizeInterval::from_precision(188.7,1e-4) | FontSizeInterval::from_precision(0.3,1e-4)
    ;"select area")]
    #[test_case(RelativeFontSizeInterval::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(800.0,30.0,2000.0,340.0)))
    ), FontSizeInterval::empty();"no result")]
    fn contextualize_fontsizeinterval(rfs: RelativeFontSizeInterval, exp_fs: FontSizeInterval) {
        let fs=rfs.contextualize(&LINES);
        assert_eq!(fs.atoms(),exp_fs.atoms());
    }

    #[test_case(RelativeTextSet::from_selection(
        PdfLineSelection::from_font(Absolute(FontSet::new("A")))
    ), TextSet::new("^text$") | TextSet::new("^with$")  | TextSet::new("^same$") | TextSet::new("^font$")
    ;"select font")]
    #[test_case(RelativeTextSet::from_selection(
        PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.14,1.8)))
    ), TextSet::new("^text$") | TextSet::new("^with$") | TextSet::new("^similar$")
    ;"select font size")]
    #[test_case(RelativeTextSet::from_selection(
        PdfLineSelection::from_text(Absolute(TextSet::new("i")))
    ), TextSet::new("^with$") | TextSet::new("^similar$") | TextSet::new("^size$") | TextSet::new("^size$")
    ;"select text")]
    #[test_case(RelativeTextSet::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(0.0,30.0,20.0,33.0)))
    ), TextSet::new("^same$") | TextSet::new("^font$") | TextSet::new("^-----$")
    ;"select area")]
    #[test_case(RelativeTextSet::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(800.0,30.0,2000.0,340.0)))
    ), TextSet::empty();"no result")]
    fn contextualize_textset(rt: RelativeTextSet, exp_t: TextSet) {
        let t=rt.contextualize(&LINES);
        assert_eq!(t.ast(),exp_t.ast());
    }


    #[test_case(RelativeArea::from_selection(
        PdfLineSelection::from_font(Absolute(FontSet::new("A")))
    ), Area::new(10.0,10.0,15.0,11.0) | Area::new(10.0,11.0,15.0,12.0) | Area::new(10.0,11.0,15.0,12.0) | Area::new(10.0,30.0,15.0,31.0) | Area::new(10.0,31.0,15.0,32.0)
    ;"select font")]
    #[test_case(RelativeArea::from_selection(
        PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.14,1.8)))
    ), Area::new(10.0,12.0,15.0,13.0) | Area::new(10.0,10.0,15.0,11.0) | Area::new(10.0,11.0,15.0,12.0)
    ;"select font size")]
    #[test_case(RelativeArea::from_selection(
        PdfLineSelection::from_text(Absolute(TextSet::new("size")))
    ), Area::new(10.0,14.0,15.0,15.0) | Area::new(10.0,34.0,15.0,35.0)
    ;"select text")]
    #[test_case(RelativeArea::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(0.0,30.0,20.0,33.0)))
    ), Area::new(10.0,30.0,15.0,31.0) | Area::new(10.0,31.0,15.0,32.0) | Area::new(10.0,32.0,15.0,33.0)
    ;"select area")]
    #[test_case(RelativeArea::MoveWindow{
        target: Box::new(PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2")))),
        vec: (1.0,-0.6),
        width_mult: 1.5,
        height_mult: 0.5
    },Area::new(65.0,18.6,110.0,20.6);"move window")]
    #[test_case(RelativeArea::Bounds{
        x0: Relative(Box::new(
            PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2")))
        )),
        y0: Absolute(60.0),
        x1: Absolute(70.0),
        y1: Absolute(300.0)
    },Area::new(65.0,60.0,70.0,300.0);"bounds left")]
    #[test_case(RelativeArea::Bounds{
        x0: Absolute(10.0),
        y0: Absolute(60.0),
        x1: Relative(Box::new(
            PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2")))
        )),
        y1: Absolute(300.0)
    },Area::new(10.0,60.0,35.0,300.0);"bounds right")]
    #[test_case(RelativeArea::Bounds{  
        x0: Absolute(10.0),
        y0: Relative(Box::new(
            PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2")))
        )),
        x1: Absolute(60.0),
        y1: Absolute(300.0)
    },Area::new(10.0,25.0,60.0,300.0);"bounds up")]
    #[test_case(RelativeArea::Bounds{  
        x0: Absolute(10.0),
        y0: Absolute(3.0),
        x1: Absolute(60.0),
        y1: Relative(Box::new(
            PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2")))
        ))
    },Area::new(10.0,3.0,60.0,21.0);"bounds down")]  
    #[test_case(RelativeArea::from_selection(
        PdfLineSelection::from_area(Absolute(Area::new(800.0,30.0,2000.0,340.0)))
    ), Area::empty();"no result")]
    fn contextualize_area(ra: RelativeArea, exp_a: Area) {
        let a=ra.contextualize(&LINES);
        assert_eq!(a.atoms(),exp_a.atoms());
    }

}