use crate::commons::sets::{DisjointAtomsSet,Container,Overlappable,AtomOperations,SetRelation,CompoundAtomOperationRes};
use crate::commons::geometry::{Limits};

impl Container for Limits {
    type Elem = f32;
    fn contains(&self,x: &f32) -> bool {
        let (a,b) = self.as_tuple();
        a <= *x && *x <= b
    }
}

impl Overlappable<Self> for Limits {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a0>b1 || b0>a1 {
            Disjoint
        } else if a0==b0 && a1==b1 {
            Equal
        } else if b0<=a0 && a1<=b1 {
            Subset
        } else if a0<=b0 && b1<=b0 {
            Superset
        } else {
            Overlapping
        }
    }
}


pub enum SubtractOverlappingLimitsRes {
    One(Limits)
}
pub enum SubtractSubsetLimitsRes {
    Two(Limits,Limits)
}
// pub enum UnionOverlappingLimitsRes {
//     One(Limits)
// }
pub enum IntersectOverlappingLimitsRes {
    One(Limits)
}

impl Into<CompoundAtomOperationRes<Limits>> for SubtractOverlappingLimitsRes {
    fn into(self) -> CompoundAtomOperationRes<Limits> {
        use CompoundAtomOperationRes::*;
        match self {
            Self::One(a) => One(a)
        }
    }
}

impl Into<CompoundAtomOperationRes<Limits>> for SubtractSubsetLimitsRes {
    fn into(self) -> CompoundAtomOperationRes<Limits> {
        use CompoundAtomOperationRes::*;
        match self {
            Self::Two(a,b) => Two(a,b)
        }
    }
}

impl Into<CompoundAtomOperationRes<Limits>> for IntersectOverlappingLimitsRes {
    fn into(self) -> CompoundAtomOperationRes<Limits> {
        use CompoundAtomOperationRes::*;
        match self {
            Self::One(a) => One(a)
        }
    }
}

impl AtomOperations for Limits {
    type SubtractSubsetRes = SubtractSubsetLimitsRes;
    type SubtractOverlappingRes = SubtractOverlappingLimitsRes;
    type IntersectOverlappingRes = IntersectOverlappingLimitsRes;
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
    // fn union_overlapping(&self,other: &Self) -> UnionOverlappingLimitsRes {
    //     use UnionOverlappingLimitsRes::*;
    //     let (a0,a1) = self.as_tuple();
    //     let (b0,b1) = other.as_tuple();
    //     if a0<b0 {
    //         One(Limits::new(a0,b1))
    //     } else {
    //         One(Limits::new(b0,a1))
    //     }
    // }
}


type Interval = DisjointAtomsSet<Limits,f32>;
type FontSizeSet = Interval;

// use crate::commons::sets::{Container,Set,AstNode,SetOps,Overlappable,SetRelation};
// use crate::commons::geometry::Limits;

// type FontSizeAstLeaf = Limits;



// impl Overlappable for Limits {
//     fn set_relation(&self,other: &Self) -> SetRelation {
//         use SetRelation::*;
//         let (a0,a1) = self.as_tuple();
//         let (b0,b1) = self.as_tuple();
//         if a0>b1 || b0>a1 {
//             Disjoint
//         } else if a0==b0 && a1==b1 {
//             Equal
//         } else if (b0<=a0 && a1<=b1) {
//             Subset
//         } else if (a0<=b0 && b1<=b0) {
//             Superset
//         } else {
//             Overlapping
//         }
//     }
// }

// impl FontSizeSet {
//     pub fn new(a: f32, b: f32) -> Self {
//         Self(AstNode::Leaf(Limits::new(a,b)))
//     }
// }


// pub type FontSizeSet = Set<Limits,f32>;
// pub type FontSizeInterval = FontSizeSet;

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use pretty_assertions::assert_eq;
//     use test_case::test_case;
//     #[test]
//     fn new_fontsizeset() {
//         let (a,b) = (0.2,0.4);
//         let res = (0.2,0.4);
//         let Set(AstNode::Leaf(interval)) = FontSizeSet::new(a,b) else {
//             panic!("Expected have to be a FontSizeSet with just one leaf")
//         };
//         assert_eq!(interval.as_tuple(),res);
//     }

//     #[test]
//     fn element_in_leafset() {
//         let interval=Limits::build(20.0,50.0).unwrap();
//         let x=30.5;
//         assert!(interval.contains(&x));
//     }
//     #[test_case(10.5;"too little")]
//     #[test_case(55.5;"too big")]
//     fn element_not_in_leafset(x: f32) {
//         let interval=Limits::build(20.0,50.0).unwrap();
//         assert!(!interval.contains(&x));
//     }
// }