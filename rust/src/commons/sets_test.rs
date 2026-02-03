// use super::{Container,SetRelation,Overlappable}
use std::iter;
use std::ops::{BitOr,BitAnd,Div};

use crate::commons::geometry::{Limits,Rectangle};

pub enum SetRelation {
    Overlapping,
    Subset,
    Superset,
    Disjoint,
    Equal
}


pub enum LimitsOperationRes {
    EmptySet,
    Both,
    Lhs,
    Rhs,
    One(Limits),
    Two(Limits,Limits)
}


pub enum RectangleOperationRes {
    EmptySet,
    Both,
    Lhs,
    Rhs,
    One(Rectangle),
    Two(Rectangle,Rectangle),
    Three(Rectangle,Rectangle,Rectangle),
    Four(Rectangle,Rectangle,Rectangle,Rectangle)
}

pub enum FontOperationRes{
    EmptySet,
    Both,
    Lhs,
    Rhs,
    Zero
}


pub enum SubtractOverlappingLimitsRes {
    One(Limits)
}
pub enum SubtractSubsetLimitsRes {
    Two(Limits,Limits)
}
pub enum UnionOverlappingLimitsRes {
    One(Limits)
}
pub enum IntersectOverlappingLimitsRes {
    One(Limits)
}

pub enum SubtractOverlappingRectanglesRes {
    One(Rectangle),
    Two(Rectangle,Rectangle),
    Three(Rectangle,Rectangle,Rectangle)
}
pub enum SubtractSubsetRectanglesRes {
    Four(Rectangle,Rectangle,Rectangle,Rectangle)
}
pub enum UnionOverlappingRectanglesRes {
    Two(Rectangle,Rectangle),
    Three(Rectangle,Rectangle,Rectangle),
    Four(Rectangle,Rectangle,Rectangle,Rectangle)
}
pub enum IntersectOverlappingRectanglesRes {
    One(Rectangle)
}

pub enum SubtractOverlappingFontsRes {
    Zero
}
pub enum SubtractSubsetFontsRes {
    Zero
}
pub enum UnionOverlappingFontsRes {
    Zero
}
pub enum IntersectOverlappingFontsRes {
    Zero
}



impl Limits {
    // type Elem = f32;
    fn contains(&self,x: &f32) -> bool {
        let (a,b) = self.as_tuple();
        a <= *x && *x <= b
    }
}


pub struct LimitsSet(Vec<Limits>);
impl LimitsSet {
    fn from_atom(a: Limits) -> Self {
        Self(vec![a])
    }
}
impl Limits {
    fn subtract_subset(&self,other: &Self) -> SubtractSubsetLimitsRes {
        use SubtractSubsetLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        Two(Limits::new(a0,b0),Limits::new(b1,a1))
    }
    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingLimitsRes {
        use SubtractOverlappingLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a1>=b0 {
            One(Limits::new(a0,b0))
        } else {
            One(Limits::new(b1,a1))
        }
    }
    fn intersect_overlapping(&self,other: &Self) -> IntersectOverlappingLimitsRes {
        use IntersectOverlappingLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a1>=b0 {
            One(Limits::new(b0,a1))
        } else {
            One(Limits::new(a0,b1))
        }
    }
    fn union_overlapping(&self,other: &Self) -> UnionOverlappingLimitsRes {
        use UnionOverlappingLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a0<b0 {
            One(Limits::new(a0,b1))
        } else {
            One(Limits::new(b0,a1))
        }
    }
}
impl Limits {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = self.as_tuple();
        if a0>b1 || b0>a1 {
            Disjoint
        } else if a0==b0 && a1==b1 {
            Equal
        } else if (b0<=a0 && a1<=b1) {
            Subset
        } else if (a0<=b0 && b1<=b0) {
            Superset
        } else {
            Overlapping
        }
    }
}
impl BitOr<Self> for &Limits {
    type Output = LimitsOperationRes;
    fn bitor(self,other: Self) -> LimitsOperationRes {
        use LimitsOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => match self.union_overlapping(other) {
                UnionOverlappingLimitsRes::One(a) => One(a)
            },
            Disjoint => Both,
        }
    }
}
impl BitAnd<Self> for &Limits {
    type Output = LimitsOperationRes;
    fn bitand(self,other: Self) -> LimitsOperationRes {
        use LimitsOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Subset => Lhs,
            Superset => Rhs,
            Overlapping => match self.intersect_overlapping(other) {
                IntersectOverlappingLimitsRes::One(a) => One(a)
            },
            Disjoint => EmptySet,
        }
    }
}
impl Div<Self> for &Limits {
    type Output = LimitsOperationRes;
    fn div(self,other: Self) -> LimitsOperationRes {
        use LimitsOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => EmptySet,
            Subset => match self.subtract_subset(other) {
                SubtractSubsetLimitsRes::Two(a,b) => Two(a,b)
            },
            Overlapping => match self.subtract_overlapping(other) {
                SubtractOverlappingLimitsRes::One(a) => One(a)
            },
            Disjoint => Lhs,
        }
    }
}

impl LimitsSet {
    fn atom_union(mut self, other: Limits) -> Self {
        use LimitsOperationRes::*;
        let mut other_pieces = Vec::with_capacity(3 * self.0.len());
        other_pieces.push(other);
        
        for a in &self.0 {
            let mut i = 0;
            while i < other_pieces.len() {
                match &other_pieces[i] / a {
                    EmptySet => {
                        other_pieces.remove(i);
                    },
                    Lhs => i += 1,
                    One(new_limit) => {
                        other_pieces[i] = new_limit;
                        i += 1;
                    },
                    Two(first, second) => {
                        other_pieces[i] = first;
                        other_pieces.insert(i + 1, second);
                        i += 2;
                    },
                    _ => unreachable!("Invalid operation result in LimitsSet bitor"),
                }
            }
        }
        self.0.extend(other_pieces);
        self
    }
}

impl LimitsSet {
    fn atom_intersection(mut self, other: Limits) -> Self {
        use LimitsOperationRes::*;
        let mut i = 0;
        while i < self.0.len() {
            match &self.0[i] & &other {
                EmptySet => {
                    self.0.remove(i);
                },
                Lhs => {
                    i += 1;
                },
                Rhs => {
                    self.0[i] = other;
                    i += 1;
                },
                One(new_limit) => {
                    self.0[i] = new_limit;
                    i += 1;
                },
                _ => unreachable!("Invalid operation result in LimitsSet atom_intersection"),
            }
        }
        self
    }
}


impl LimitsSet {
    fn atom_subtraction(mut self, other: Limits) -> Self {
        use LimitsOperationRes::*;
        let mut i = 0;
        while i < self.0.len() {
            match &self.0[i] / &other {
                EmptySet => {
                    self.0.remove(i);
                },
                Lhs => {
                    i += 1;
                },
                One(new_limit) => {
                    self.0[i] = new_limit;
                    i += 1;
                },
                Two(first, second) => {
                    self.0[i] = first;
                    self.0.insert(i + 1, second);
                    i += 2;
                },
                _ => unreachable!("Invalid operation result in LimitsSet atom_intersection"),
            }
        }
        self
    }
}     

impl BitOr<Self> for LimitsSet {
    type Output = Self;
    fn bitor(mut self,other: Self) -> Self {
        for o in other.0 {
            self=self.atom_union(o)
        }
        self
    }
}
impl BitAnd<Self> for LimitsSet {
    type Output = Self;
    fn bitand(mut self,other: Self) -> Self {
        for o in other.0 {
            self=self.atom_intersection(o)
        }
        self
    }
}
impl Div<Self> for LimitsSet {
    type Output = Self;
    fn div(mut self,other: Self) -> Self {
        for o in other.0 {
            self=self.atom_subtraction(o)
        }
        self
    }
}

impl LimitsSet {
    // type Elem = f32;
    fn contains(&self,e: &f32) -> bool {
        for a in &self.0 {
            if a.contains(e) {
                return true;
            }
        }
        false
    }
}



pub struct RectSet(Vec<Rectangle>);
impl RectSet {
    fn from_atom(a: Rectangle) -> Self {
        Self(vec![a])
    }
}


enum RectOverlapping {
    SmallerLeft,
    SmallerRight,
    SmallerTop,
    SmallerBottom,
    BiggerLeft,
    BiggerRight,
    BiggerTop,
    BiggerBottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight
}
impl Rectangle {
    fn type_overlap(&self,other: &Self) -> RectOverlapping {
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        if x0<=a0 {
            // BiggerRight SmallerRight TopRight BottomRight SmallerTop SmallerBottom
            if x1 <= b1 {
                // BiggerRight SmallerRight TopRight BottomRight
                if y0<=b0 {
                    // SmallerRight BottomRight
                    if b1 <= y1 {
                        SmallerRight
                    } else {
                        BottomRight
                    }
                } else {
                    // BiggerRight TopRight
                    if b0 <= y0 {
                        TopRight
                    } else {
                        BiggerRight
                    }
                }
            } else {
                // SmallerTop SmallerBottom
                if y0 <= b1 {
                    SmallerTop
                } else {
                    SmallerBottom
                }
            }
        } else {
            // BiggerLeft SmallerLeft TopLeft BottomLeft BiggerTop BiggerBottom
            if x1<=a1 {
                // BiggerLeft SmallerLeft TopLeft BottomLeft
                if y0<=b0 {
                    // SmallerLeft BottomLeft
                    if b1 <= y1 {
                        SmallerLeft
                    } else {
                        BottomLeft
                    }
                } else {
                    // BiggerLeft TopLeft
                    if b0 <= y0 {
                        TopLeft
                    } else {
                        BiggerLeft
                    }
                }
            } else {
                // BiggerTop BiggerBottom
                if y0 <= b1 {
                    BiggerTop
                } else {
                    BiggerBottom
                }
            }
        }
    }
}



impl Rectangle {
    
    fn subtract_subset(&self,other: &Self) -> SubtractSubsetRectanglesRes {
        use SubtractSubsetRectanglesRes::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        let h: (f32,f32,f32,f32);
        let v: (f32,f32,f32,f32);
        if a0<x0 || b0<y0 || a1<x1 || b1<y1 {
            h=(a0,x0,x1,a1);
            v=(b0,y0,y1,b1);
        } else {
            h=(x0,a0,a1,x1);
            v=(y0,b0,b1,y1);
        }
        Four(
            Self::new(h.0,v.0,h.2,v.1),
            Self::new(h.0,v.1,h.1,v.3),
            Self::new(h.1,v.2,h.3,v.3),
            Self::new(h.2,v.0,h.3,v.2)
        )
    }
    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingRectanglesRes {
        use SubtractOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        match self.type_overlap(other) {
            SmallerLeft => Three(
                Self::new(x0,y0,x1,b0),
                Self::new(a1,b0,x1,y1),
                Self::new(x0,b1,a1,y1)
            ),
            SmallerRight => Three(
                Self::new(x0,b1,x1,y1),
                Self::new(x0,y0,a0,b1),
                Self::new(a0,y0,x1,b0)
            ),
            SmallerTop => Three(
                Self::new(a1,y0,x1,y1),
                Self::new(x0,b1,a1,y1),
                Self::new(x0,y0,a0,b1)
            ),
            SmallerBottom => Three(
                Self::new(x0,y0,a0,y1),
                Self::new(a0,y0,x1,b0),
                Self::new(a1,b0,x1,y1)
            ),
            BiggerLeft => One(Self::new(a1,y0,x1,y1)),
            BiggerRight => One(Self::new(x0,y0,a0,y1)),
            BiggerTop => One(Self::new(x0,b1,x1,y1)),
            BiggerBottom => One(Self::new(x0,y0,x1,b0)),
            TopLeft => Two(
                Self::new(a1,y0,x1,y1),
                Self::new(x0,b1,a1,y1)
            ),
            TopRight => Two(
                Self::new(x0,y0,x1,b0),
                Self::new(a1,b0,x1,y1)
            ),
            BottomRight => Two(
                Self::new(x0,y0,a0,y1),
                Self::new(a0,y0,x1,b0)
            ),
            BottomLeft => Two(
                Self::new(x0,b1,x1,y1),
                Self::new(x0,y0,a0,b1)
            )
        }
    }

    fn intersect_overlapping(&self,other: &Self) -> IntersectOverlappingRectanglesRes {
        use IntersectOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        match self.type_overlap(other) {
            SmallerLeft => One(Self::new(x0,b0,a1,b1)),
            SmallerRight => One(Self::new(a0,b0,x1,b1)),
            SmallerTop => One(Self::new(a0,y0,a1,b1)),
            SmallerBottom => One(Self::new(a0,b0,a1,y1)),
            BiggerLeft => One(Self::new(x0,y0,a1,y1)),
            BiggerRight => One(Self::new(a0,y0,x1,y1)),
            BiggerTop => One(Self::new(x0,y0,x1,b1)),
            BiggerBottom => One(Self::new(x0,b0,x1,y1)),
            TopLeft => One(Self::new(x0,y0,a1,b1)),
            TopRight => One(Self::new(x0,b0,a1,y1)),
            BottomRight => One(Self::new(a0,b0,x1,y1)),
            BottomLeft => One(Self::new(a0,y0,x1,b1))
        }
    }
    fn union_overlapping(&self,other: &Self) -> UnionOverlappingRectanglesRes {
        use UnionOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let a = Rectangle::new(x0,y0,x1,y1);
        match self.subtract_overlapping(other) {
            SubtractOverlappingRectanglesRes::One(b) => Two(a,b),
            SubtractOverlappingRectanglesRes::Two(b,c) => Three(a,b,c),
            SubtractOverlappingRectanglesRes::Three(b,c,d) => Four(a,b,c,d),
        }
    }
}
impl Rectangle {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = self.as_tuple();
        if (
            (x0>a1 || x1<a0) || (a0>x1 || a1<x0)
        ) || (
            (y0>b1 || y1<b0) || (b0>y1 || b1<y0)
        ){
            Disjoint
        } else if (x0,y0,x1,y1) == (a0,b0,a1,b1) {
            Equal
        } else if (a0<=x0 && x1<=a1 && b0<=y0 && y1<=b1) {
            Subset
        } else if (x0<=a0 && a1<=x1 && y0<=b0 && b1<=y1) {
            Superset
        } else {
            Overlapping
        }
    }
}

impl BitOr<Self> for &Rectangle {
    type Output = RectangleOperationRes;
    fn bitor(self,other: Self) -> RectangleOperationRes {
        use RectangleOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => match self.union_overlapping(other) {
                UnionOverlappingRectanglesRes::Two(a,b) => Two(a,b),
                UnionOverlappingRectanglesRes::Three(a,b,c) => Three(a,b,c),
                UnionOverlappingRectanglesRes::Four(a,b,c,d) => Four(a,b,c,d)
            },
            Disjoint => Both,
        }
    }
}
impl BitAnd<Self> for &Rectangle {
    type Output = RectangleOperationRes;
    fn bitand(self,other: Self) -> RectangleOperationRes {
        use RectangleOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Subset => Lhs,
            Superset => Rhs,
            Overlapping => match self.intersect_overlapping(other) {
                IntersectOverlappingRectanglesRes::One(a) => One(a)
            },
            Disjoint => EmptySet,
        }
    }
}
impl Div<Self> for &Rectangle {
    type Output = RectangleOperationRes;
    fn div(self,other: Self) -> RectangleOperationRes {
        use RectangleOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => EmptySet,
            Subset => match self.subtract_subset(other) {
                SubtractSubsetRectanglesRes::Four(a,b,c,d) => Four(a,b,c,d)
            },
            Overlapping => match self.subtract_overlapping(other) {
                SubtractOverlappingRectanglesRes::One(a) => One(a),
                SubtractOverlappingRectanglesRes::Two(a,b) => Two(a,b),
                SubtractOverlappingRectanglesRes::Three(a,b,c) => Three(a,b,c)
            },
            Disjoint => Lhs,
        }
    }
}




#[derive(Clone)]
pub struct Font(String);
pub struct FontSet(Vec<Font>);
impl FontSet {
    fn from_atom(a: Font) -> Self {
        Self(vec![a])
    }
}
impl Font {
    fn subtract_subset(&self,other: &Self) -> SubtractSubsetFontsRes {
        SubtractSubsetFontsRes::Zero
    }
    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingFontsRes {
        SubtractOverlappingFontsRes::Zero
    }
    fn intersect_overlapping(&self,other: &Self) -> IntersectOverlappingFontsRes {
        IntersectOverlappingFontsRes::Zero
    }
    fn union_overlapping(&self,other: &Self) -> UnionOverlappingFontsRes {
        UnionOverlappingFontsRes::Zero
    }
}
impl Font {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        if self.0==other.0 {Equal} else {Disjoint}
    }
}

impl BitOr<Self> for &Font {
    type Output = FontOperationRes;
    fn bitor(self,other: Self) -> FontOperationRes {
        use FontOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => match self.union_overlapping(other) {
                UnionOverlappingFontsRes::Zero => Zero
            },
            Disjoint => Both,
        }
    }
}
impl BitAnd<Self> for &Font {
    type Output = FontOperationRes;
    fn bitand(self,other: Self) -> FontOperationRes {
        use FontOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Subset => Lhs,
            Superset => Rhs,
            Overlapping => match self.intersect_overlapping(other) {
                IntersectOverlappingFontsRes::Zero => Zero
            },
            Disjoint => EmptySet,
        }
    }
}
impl Div<Self> for &Font {
    type Output = FontOperationRes;
    fn div(self,other: Self) -> FontOperationRes {
        use FontOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => EmptySet,
            Subset => match self.subtract_subset(other) {
                SubtractSubsetFontsRes::Zero => Zero
            },
            Overlapping => match self.subtract_overlapping(other) {
                SubtractOverlappingFontsRes::Zero => Zero
            },
            Disjoint => Lhs,
        }
    }
}
