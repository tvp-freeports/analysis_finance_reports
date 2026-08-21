use ordered_float::OrderedFloat;
use std::cmp::max;
use crate::commons::sets::{
    DisjointAtomsSet,
    Container,
    Overlappable,
    AtomOperations,
    AtomAlgebra,
    SetRelation,
    CompoundAtomOperationRes
};
use crate::commons::geometry::{PositiveLimits};

impl Container for PositiveLimits {
    type Elem = f32;
    fn contains(&self,x: &f32) -> bool {
        let (a,b) = self.as_tuple();
        a <= *x && *x <= b
    }
}

impl Overlappable<Self> for PositiveLimits {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a0>=b1 || b0>=a1 {
            Disjoint
        } else if a0==b0 && a1==b1 {
            Equal
        } else if b0<=a0 && a1<=b1 {
            Subset
        } else if a0<=b0 && b1<=a1 {
            Superset
        } else {
            Overlapping
        }
    }
}


pub enum SubtractOverlappingPositiveLimitsRes {
    One(PositiveLimits)
}
pub enum SubtractSubsetPositiveLimitsRes {
    One(PositiveLimits),
    Two(PositiveLimits,PositiveLimits)
}

pub enum IntersectOverlappingPositiveLimitsRes {
    One(PositiveLimits)
}

impl From<SubtractOverlappingPositiveLimitsRes> for CompoundAtomOperationRes<PositiveLimits> {
    fn from(val: SubtractOverlappingPositiveLimitsRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            SubtractOverlappingPositiveLimitsRes::One(a) => One(a)
        }
    }
}

impl From<SubtractSubsetPositiveLimitsRes> for CompoundAtomOperationRes<PositiveLimits> {
    fn from(val: SubtractSubsetPositiveLimitsRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            SubtractSubsetPositiveLimitsRes::One(a) => One(a),
            SubtractSubsetPositiveLimitsRes::Two(a,b) => Two(a,b)
        }
    }
}

impl From<IntersectOverlappingPositiveLimitsRes> for CompoundAtomOperationRes<PositiveLimits> {
    fn from(val: IntersectOverlappingPositiveLimitsRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            IntersectOverlappingPositiveLimitsRes::One(a) => One(a)
        }
    }
}

impl AtomOperations for PositiveLimits {
    type SubtractSubsetRes = SubtractSubsetPositiveLimitsRes;
    type SubtractOverlappingRes = SubtractOverlappingPositiveLimitsRes;
    type IntersectOverlappingRes = IntersectOverlappingPositiveLimitsRes;
    fn subtract_subset(&self,other: &Self) -> SubtractSubsetPositiveLimitsRes {
        use SubtractSubsetPositiveLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if b0==a0 {
            One(PositiveLimits::new(b1,a1))
        } else if a1==b1 {
            One(PositiveLimits::new(a0,b0))
        } else {
            Two(PositiveLimits::new(a0,b0),PositiveLimits::new(b1,a1))
        }
        
    }
    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingPositiveLimitsRes {
        use SubtractOverlappingPositiveLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if b1>=a1 {
            One(PositiveLimits::new(a0,b0))
        } else {
            One(PositiveLimits::new(b1,a1))
        }
    }
    fn intersect_overlapping(&self,other: &Self) -> IntersectOverlappingPositiveLimitsRes {
        use IntersectOverlappingPositiveLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if b1>=a1 {
            One(PositiveLimits::new(b0,a1))
        } else {
            One(PositiveLimits::new(a0,b1))
        }
    }
    fn union_overlapping(&self,other: &Self) -> CompoundAtomOperationRes<Self> {
        use CompoundAtomOperationRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a0<b0 {
            One(PositiveLimits::new(a0,b1))
        } else {
            One(PositiveLimits::new(b0,a1))
        }
    }
}

impl AtomAlgebra for PositiveLimits {}

type Interval = DisjointAtomsSet<PositiveLimits,f32>;
pub type FontSizeSet = Interval;
pub type FontSizeInterval = FontSizeSet;

impl FontSizeInterval {
    pub fn new(a:f32,b:f32) -> Self {
        Self::from_atom(PositiveLimits::new(a,b))
    }
    pub fn from_precision(c:f32,prec:f32) -> Self {
        let a = max(OrderedFloat(0.0),OrderedFloat(c-prec)).into_inner();
        Self::from_atom(PositiveLimits::new(a,c+prec))
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;
    use std::collections::HashSet;
    fn new_fontset() {
        let (a,b) = (6.0,70.0);
        let mut set = HashSet::new();
        set.insert(PositiveLimits::new(6.0,70.0));
        assert_eq!(FontSizeInterval::new(6.0,70.0).atoms(),&set);
    }
    fn fontset_from_precision() {
        let (c,p) = (60.0,5.0);
        let mut set = HashSet::new();
        set.insert(PositiveLimits::new(c-p,c+p));
        assert_eq!(FontSizeInterval::from_precision(c,p).atoms(),&set);
    }

    #[test_case(PositiveLimits::new(20.0,50.0),30.5;"common")]
    #[test_case(PositiveLimits::new(20.0,50.0),50.0;"touch right")]
    #[test_case(PositiveLimits::new(20.0,50.0),20.0;"touch left")]
    fn element_in_atom(interval: PositiveLimits, x: f32) {
        assert!(interval.contains(&x));
    }
    #[test_case(10.5;"too little")]
    #[test_case(55.5;"too big")]
    fn element_not_in_atom(x: f32) {
        let interval=PositiveLimits::build(20.0,50.0).unwrap();
        assert!(!interval.contains(&x));
    }

    use SetRelation::*;
    #[test_case(PositiveLimits::new(2.0,5.5),Equal,PositiveLimits::new(2.0,5.5);"equal")]
    #[test_case(PositiveLimits::new(1.9,5.8),Superset,PositiveLimits::new(2.0,5.5);"superset")]
    #[test_case(PositiveLimits::new(2.0,5.8),Superset,PositiveLimits::new(2.0,5.5);"superset left touch")]
    #[test_case(PositiveLimits::new(1.9,5.5),Superset,PositiveLimits::new(2.0,5.5);"superset right touch")]
    #[test_case(PositiveLimits::new(3.0,3.5),Subset,PositiveLimits::new(2.0,5.5);"subset")]
    #[test_case(PositiveLimits::new(2.0,3.5),Subset,PositiveLimits::new(2.0,5.5);"subset left touch")]
    #[test_case(PositiveLimits::new(3.0,5.5),Subset,PositiveLimits::new(2.0,5.5);"subset right touch")]
    #[test_case(PositiveLimits::new(2.0,5.5),Overlapping,PositiveLimits::new(5.0,50.5);"overlapping")]
    #[test_case(PositiveLimits::new(2.0,5.5),Disjoint,PositiveLimits::new(20.0,50.5);"disjoint")]
    #[test_case(PositiveLimits::new(2.0,5.5),Disjoint,PositiveLimits::new(5.5,50.5);"disjoint touch")]
    fn set_relation(a: PositiveLimits, rel: SetRelation, b: PositiveLimits) {
        assert_eq!(a.set_relation(&b),rel);
    }

    use CompoundAtomOperationRes::*;
    #[test_case(PositiveLimits::new(2.0,5.5),PositiveLimits::new(5.0,50.5),One(PositiveLimits::new(2.0,5.0));"right")]
    #[test_case(PositiveLimits::new(5.0,53.5),PositiveLimits::new(2.0,5.5),One(PositiveLimits::new(5.5,53.5));"left")]
    fn subtract_overlapping(a: PositiveLimits, b: PositiveLimits, res: CompoundAtomOperationRes<PositiveLimits>) {
        match (a.subtract_overlapping(&b).into(),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected variant")
        }
    }
    #[test_case(PositiveLimits::new(2.0,5.5),PositiveLimits::new(5.0,50.5),One(PositiveLimits::new(5.0,5.5));"right")]
    #[test_case(PositiveLimits::new(5.1,53.5),PositiveLimits::new(2.0,5.6),One(PositiveLimits::new(5.1,5.6));"left")]
    fn intersect_overlapping(a: PositiveLimits, b: PositiveLimits, res: CompoundAtomOperationRes<PositiveLimits>) {
        match (a.intersect_overlapping(&b).into(),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected variant")
        }
    }
    #[test_case(PositiveLimits::new(2.0,5.5),PositiveLimits::new(5.0,50.5),One(PositiveLimits::new(2.0,50.5));"right")]
    #[test_case(PositiveLimits::new(5.1,53.5),PositiveLimits::new(2.2,5.6),One(PositiveLimits::new(2.2,53.5));"left")]
    fn union_overlapping(a: PositiveLimits, b: PositiveLimits, res: CompoundAtomOperationRes<PositiveLimits>) {
        match (a.union_overlapping(&b),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected variant")
        }
    }

    #[test_case(PositiveLimits::new(30.6,40.2),PositiveLimits::new(33.6,36.1),Two(
        PositiveLimits::new(30.6,33.6),PositiveLimits::new(36.1,40.2)
    );"common")]
    #[test_case(PositiveLimits::new(30.6,40.2),PositiveLimits::new(30.6,36.1),One(
        PositiveLimits::new(36.1,40.2)
    );"left touch")]
    #[test_case(PositiveLimits::new(30.6,40.2),PositiveLimits::new(33.6,40.2),One(
        PositiveLimits::new(30.6,33.6)
    );"right touch")]
    fn subtract_subset(a: PositiveLimits, b: PositiveLimits, res: CompoundAtomOperationRes<PositiveLimits>) {
        match (a.subtract_subset(&b).into(),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            (Two(ra,rb),Two(ea,eb)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
            },
            _ => panic!("Result doesn't have the expected variant")
        }
    }
}
