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


pub enum SubtractOverlappingLimitsRes {
    One(Limits)
}
pub enum SubtractSubsetLimitsRes {
    One(Limits),
    Two(Limits,Limits)
}

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
            Self::One(a) => One(a),
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
        if b0==a0 {
            One(Limits::new(b1,a1))
        } else if a1==b1 {
            One(Limits::new(a0,b0))
        } else {
            Two(Limits::new(a0,b0),Limits::new(b1,a1))
        }
        
    }
    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingLimitsRes {
        use SubtractOverlappingLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if b1>=a1 {
            One(Limits::new(a0,b0))
        } else {
            One(Limits::new(b1,a1))
        }
    }
    fn intersect_overlapping(&self,other: &Self) -> IntersectOverlappingLimitsRes {
        use IntersectOverlappingLimitsRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if b1>=a1 {
            One(Limits::new(b0,a1))
        } else {
            One(Limits::new(a0,b1))
        }
    }
    fn union_overlapping(&self,other: &Self) -> CompoundAtomOperationRes<Self> {
        use CompoundAtomOperationRes::*;
        let (a0,a1) = self.as_tuple();
        let (b0,b1) = other.as_tuple();
        if a0<b0 {
            One(Limits::new(a0,b1))
        } else {
            One(Limits::new(b0,a1))
        }
    }
}


type Interval = DisjointAtomsSet<Limits,f32>;
type FontSizeSet = Interval;



#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;
    #[test_case(Limits::new(20.0,50.0),30.5;"common")]
    #[test_case(Limits::new(20.0,50.0),50.0;"touch right")]
    #[test_case(Limits::new(20.0,50.0),20.0;"touch left")]
    fn element_in_atom(interval: Limits, x: f32) {
        assert!(interval.contains(&x));
    }
    #[test_case(10.5;"too little")]
    #[test_case(55.5;"too big")]
    fn element_not_in_atom(x: f32) {
        let interval=Limits::build(20.0,50.0).unwrap();
        assert!(!interval.contains(&x));
    }

    use SetRelation::*;
    #[test_case(Limits::new(2.0,5.5),Equal,Limits::new(2.0,5.5);"equal")]
    #[test_case(Limits::new(1.9,5.8),Superset,Limits::new(2.0,5.5);"superset")]
    #[test_case(Limits::new(2.0,5.8),Superset,Limits::new(2.0,5.5);"superset left touch")]
    #[test_case(Limits::new(1.9,5.5),Superset,Limits::new(2.0,5.5);"superset right touch")]
    #[test_case(Limits::new(3.0,3.5),Subset,Limits::new(2.0,5.5);"subset")]
    #[test_case(Limits::new(2.0,3.5),Subset,Limits::new(2.0,5.5);"subset left touch")]
    #[test_case(Limits::new(3.0,5.5),Subset,Limits::new(2.0,5.5);"subset right touch")]
    #[test_case(Limits::new(2.0,5.5),Overlapping,Limits::new(5.0,50.5);"overlapping")]
    #[test_case(Limits::new(2.0,5.5),Disjoint,Limits::new(20.0,50.5);"disjoint")]
    #[test_case(Limits::new(2.0,5.5),Disjoint,Limits::new(5.5,50.5);"disjoint touch")]
    fn set_relation(a: Limits, rel: SetRelation, b: Limits) {
        assert_eq!(a.set_relation(&b),rel);
    }

    use CompoundAtomOperationRes::*;
    #[test_case(Limits::new(2.0,5.5),Limits::new(5.0,50.5),One(Limits::new(2.0,5.0));"right")]
    #[test_case(Limits::new(5.0,53.5),Limits::new(2.0,5.5),One(Limits::new(5.5,53.5));"left")]
    fn subtract_overlapping(a: Limits, b: Limits, res: CompoundAtomOperationRes<Limits>) {
        match (a.subtract_overlapping(&b).into(),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected variant")
        }
    }
    #[test_case(Limits::new(2.0,5.5),Limits::new(5.0,50.5),One(Limits::new(5.0,5.5));"right")]
    #[test_case(Limits::new(5.1,53.5),Limits::new(2.0,5.6),One(Limits::new(5.1,5.6));"left")]
    fn intersect_overlapping(a: Limits, b: Limits, res: CompoundAtomOperationRes<Limits>) {
        match (a.intersect_overlapping(&b).into(),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected variant")
        }
    }
    #[test_case(Limits::new(2.0,5.5),Limits::new(5.0,50.5),One(Limits::new(2.0,50.5));"right")]
    #[test_case(Limits::new(5.1,53.5),Limits::new(2.2,5.6),One(Limits::new(2.2,53.5));"left")]
    fn union_overlapping(a: Limits, b: Limits, res: CompoundAtomOperationRes<Limits>) {
        match (a.union_overlapping(&b),res) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected variant")
        }
    }

    #[test_case(Limits::new(30.6,40.2),Limits::new(33.6,36.1),Two(
        Limits::new(30.6,33.6),Limits::new(36.1,40.2)
    );"common")]
    #[test_case(Limits::new(30.6,40.2),Limits::new(30.6,36.1),One(
        Limits::new(36.1,40.2)
    );"left touch")]
    #[test_case(Limits::new(30.6,40.2),Limits::new(33.6,40.2),One(
        Limits::new(30.6,33.6)
    );"right touch")]
    fn subtract_subset(a: Limits, b: Limits, res: CompoundAtomOperationRes<Limits>) {
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
