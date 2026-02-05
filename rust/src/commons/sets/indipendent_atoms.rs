use super::{Container,Overlappable,SetRelation,Set,SetAlgebra};
use std::fmt::Debug;
use std::ops::{BitOr, BitAnd, Div};

#[derive(Debug)]
enum CompoundAtomOperationRes<T> {
    One(T),
    Two(T,T),
    Three(T,T,T),
    Four(T,T,T,T)
}

#[derive(Debug)]
enum AtomOperationRes<T> {
    EmptySet,
    Lhs,
    Rhs,
    Both,
    Compound(CompoundAtomOperationRes<T>)
}


trait AtomOperations: Sized {
    type SubtractSubsetRes: Into<CompoundAtomOperationRes<Self>>;
    type SubtractOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    type IntersectOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes;
    fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes;
    fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes;

}

trait AtomAlgebra: Overlappable<Self> + AtomOperations {
    fn union(&self, other: &Self) -> AtomOperationRes<Self> {
        use SetRelation::*;
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        match self.set_relation(&other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => {
                Compound(match (
                    self.subtract_overlapping(&other).into(),
                    self.intersect_overlapping(&other).into(),
                    other.subtract_overlapping(&self).into() 
                ) {
                    (One(a),One(b),One(c)) => Three(a,b,c),
                    (Two(a,b),One(c),One(d)) => Four(a,b,c,d),
                    (One(a),Two(b,c),One(d)) => Four(a,b,c,d),
                    (One(a),One(b),Two(c,d)) => Four(a,b,c,d),
                    _ => panic!("Default implementation of atom union doesn't support this combination of set subtraction and set intersection")
                })
            },
            Disjoint => Both,
        }
    }
    fn intersect(&self,other: &Self) -> AtomOperationRes<Self> {
        use SetRelation::*;
        use AtomOperationRes::*;
        match self.set_relation(&other) {
            Equal | Subset => Lhs,
            Superset => Rhs,
            Overlapping => Compound(
                self.intersect_overlapping(&other).into()
            ),
            Disjoint => EmptySet,
        }
    }
    fn subtract(&self,other: &Self) -> AtomOperationRes<Self> {
        use SetRelation::*;
        use AtomOperationRes::*;
        match self.set_relation(&other) {
            Equal | Subset => EmptySet,
            Superset => Compound(
                self.subtract_subset(&other).into()
            ),
            Overlapping => Compound(
                self.subtract_overlapping(&other).into()
            ),
            Disjoint => Lhs,
        }
    }
}


#[derive(Clone,Debug)]
struct DisjointAtomsSet<A,E>(Vec<A>)
where
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug,
    E: ?Sized
;

impl<A,E> DisjointAtomsSet<A,E>
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug,
    E: ?Sized
{
    fn from_atom(atom: A) -> Self {
        Self(vec![atom])
    }
    fn atom_union(mut self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut new_atoms = Vec::with_capacity(4 * self.0.len());
        new_atoms.push(other);
        for atm in &self.0 {
            let mut i = 0;
            while i < new_atoms.len() {
                if let SetRelation::Disjoint = new_atoms[i].set_relation(atm) {
                    i+=1;
                } else {
                    match new_atoms[i].union(atm) {
                        EmptySet => {
                            new_atoms.remove(i);
                        },
                        Lhs => i+=1,
                        Rhs => {
                            new_atoms[i] = atm.clone();
                            i+=1;
                        },
                        Both => {
                            new_atoms.insert(i + 1, atm.clone());
                            i+=1;
                        },
                        Compound(One(a)) => {
                            new_atoms[i] = a;
                            i+=1;
                        },
                        Compound(Two(a, b)) => {
                            new_atoms[i] = a;
                            new_atoms.insert(i + 1, b);
                            i+=2;
                        },
                        Compound(Three(a, b, c)) => {
                            new_atoms[i] = a;
                            new_atoms.insert(i + 1, b);
                            new_atoms.insert(i + 2, c);
                            i+=3;
                        },
                        Compound(Four(a, b, c, d)) => {
                            new_atoms[i] = a;
                            new_atoms.insert(i + 1, b);
                            new_atoms.insert(i + 2, c);
                            new_atoms.insert(i + 3, d);
                            i+=4;
                        }
                    }
                }
            }
        }
        Self(new_atoms)
    }

    fn atom_intersection(mut self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut i = 0;
        while i < self.0.len() {
            match self.0[i].intersect(&other) {
                EmptySet => {
                    self.0.remove(i);
                },
                Lhs => i += 1,
                Rhs => {
                    self.0[i] = other.clone();
                    i += 1;
                },
                Compound(One(a)) => {
                    self.0[i] = a;
                    i += 1;
                },
                Compound(Two(a, b)) => {
                    self.0[i] = a;
                    self.0.insert(i + 1, b);
                    i += 2;
                },
                Compound(Three(a, b, c)) => {
                    self.0[i] = a;
                    self.0.insert(i + 1, b);
                    self.0.insert(i + 2, c);
                    i += 3;
                },
                Compound(Four(a, b, c, d)) => {
                    self.0[i] = a;
                    self.0.insert(i + 1, b);
                    self.0.insert(i + 2, c);
                    self.0.insert(i + 3, d);
                    i += 4;
                },
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_intersection"),
            }
        }
        self
    }

    fn atom_subtraction(mut self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut i = 0;
        while i < self.0.len() {
            match self.0[i].subtract(&other) {
                EmptySet => {
                    self.0.remove(i);
                },
                Lhs => i += 1,
                Compound(One(a)) => {
                    self.0[i] = a;
                    i += 1;
                },
                Compound(Two(a, b)) => {
                    self.0[i] = a;
                    self.0.insert(i + 1, b);
                    i += 2;
                },
                Compound(Three(a, b, c)) => {
                    self.0[i] = a;
                    self.0.insert(i + 1, b);
                    self.0.insert(i + 2, c);
                    i += 3;
                },
                Compound(Four(a, b, c, d)) => {
                    self.0[i] = a;
                    self.0.insert(i + 1, b);
                    self.0.insert(i + 2, c);
                    self.0.insert(i + 3, d);
                    i += 4;
                },
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_subtraction"),
            }
        }
        self
    }
}

impl<A,E> BitOr<Self> for DisjointAtomsSet<A,E> 
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug,
    E: ?Sized
{
    type Output = Self;
    fn bitor(mut self,other: Self) -> Self {
        for o in other.0 {
            self=self.atom_union(o)
        }
        self
    }
}
impl<A,E> BitAnd<Self> for DisjointAtomsSet<A,E> 
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug,
    E: ?Sized
{
    type Output = Self;
    fn bitand(mut self,other: Self) -> Self {
        for o in other.0 {
            self=self.atom_intersection(o)
        }
        self
    }
}
impl<A,E> Div<Self> for DisjointAtomsSet<A,E> 
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug,
    E: ?Sized
{
    type Output = Self;
    fn div(mut self,other: Self) -> Self {
        for o in other.0 {
            self=self.atom_subtraction(o)
        }
        self
    }
}

impl<A,E> Container for DisjointAtomsSet<A,E>
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug,
    E: ?Sized
{
    type Elem = E;
    fn contains(&self,e: &Self::Elem) -> bool {
        for a in &self.0 {
            if a.contains(e) {
                return true;
            }
        }
        false
    }
}

impl<A,E> SetAlgebra for DisjointAtomsSet<A,E> 
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug,
    E: ?Sized
{}

impl<A,E> Set<E> for DisjointAtomsSet<A,E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug,
    E: ?Sized
{}


#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use pretty_assertions::assert_eq;
    use std::collections::HashSet;
    #[derive(Clone,Debug,PartialEq)]
    struct TestAtom(HashSet<u32>);
    impl Container for TestAtom {
        type Elem = u32;
        fn contains(&self,n: &u32) -> bool {
            self.0.contains(n)
        }
    }
    impl Overlappable<Self> for TestAtom {
        fn set_relation(&self, other: &Self) -> SetRelation {
            use SetRelation::*;
            if self==other {
                Equal
            } else if self.0.is_subset(&other.0) {
                Subset
            } else if self.0.is_superset(&other.0) {
                Superset
            } else if self.0.is_disjoint(&other.0) {
                Disjoint
            } else {
                Overlapping
            }
        }
    }
    enum TestAtomOpsRes {
        One(HashSet<u32>),
        Two(HashSet<u32>,HashSet<u32>),
        Three(HashSet<u32>,HashSet<u32>,HashSet<u32>),
        Four(HashSet<u32>,HashSet<u32>,HashSet<u32>,HashSet<u32>)
    }
    impl From<TestAtomOpsRes> for CompoundAtomOperationRes<TestAtom> {
        fn from(value: TestAtomOpsRes) -> Self {
            use TestAtomOpsRes::*;
            match value {
                One(a) => Self::One(TestAtom(a)),
                Two(a,b) => Self::Two(
                    TestAtom(a),
                    TestAtom(b)
                ),
                Three(a,b,c) => Self::Three(
                    TestAtom(a),
                    TestAtom(b),
                    TestAtom(c)
                ),
                Four(a,b,c,d) => Self::Four(
                    TestAtom(a),
                    TestAtom(b),
                    TestAtom(c),
                    TestAtom(d)
                )
            }
        }
    }
    impl AtomOperations for TestAtom {
        type SubtractOverlappingRes = TestAtomOpsRes;
        type IntersectOverlappingRes = TestAtomOpsRes;
        type SubtractSubsetRes = TestAtomOpsRes;
        fn subtract_overlapping(&self,other: &Self) -> Self::SubtractOverlappingRes {
            use TestAtomOpsRes::*;
            One(self.0.difference(&other.0).map(|x| *x).collect())
        }
        fn intersect_overlapping(&self,other: &Self) -> Self::IntersectOverlappingRes {
            use TestAtomOpsRes::*;
            let res_set: HashSet<&u32>=self.0.intersection(&other.0).collect();
            if res_set.len() == 2 {
                let mut i=res_set.into_iter();
                Two(
                    HashSet::from([*i.next().unwrap()]),
                    HashSet::from([*i.next().unwrap()])
                )
            } else if res_set.len() == 4 {
                let mut i=res_set.into_iter();
                Four(
                    HashSet::from([*i.next().unwrap()]),
                    HashSet::from([*i.next().unwrap()]),
                    HashSet::from([*i.next().unwrap()]),
                    HashSet::from([*i.next().unwrap()])
                )
            } else {
                One(res_set.into_iter().map(|x| *x).collect())
            }
        }
        fn subtract_subset(&self,other: &Self) -> Self::SubtractSubsetRes {
            use TestAtomOpsRes::*;
            One(self.0.difference(&other.0).map(|x| *x).collect())
        }
    }
    impl AtomAlgebra for TestAtom {}
    type TestSet = DisjointAtomsSet<TestAtom,u32>;
    impl TestAtom {
        fn new<const N: usize>(vec: [u32; N]) -> Self {
            Self(HashSet::from(vec))
        }
    }
    impl TestSet {
        fn new<const N: usize>(vec: [u32; N]) -> Self {
            Self::from_atom(TestAtom::new(vec))
        }
    }
    #[test]
    fn from_atom() {
        let a=TestSet::from_atom(TestAtom(HashSet::from([20,30,40])));
        assert_eq!(a.0,vec![TestAtom(HashSet::from([20,30,40]))]);
    }
    mod atom_ops {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([1,2,3]),
            Lhs; "equal"
        )]
        #[test_case(
            TestAtom::new([1,2,3,4,5]),
            TestAtom::new([1,2,3]),
            Lhs; "superset"
        )]
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([1,2,3,50,10]),
            Rhs; "subset"
        )]
        #[test_case(
            TestAtom::new([1,2,3,99,999,9,10]),
            TestAtom::new([2,3,4,5,6,99,999,9]),
            Compound(Three(
                TestAtom::new([1,10]),
                TestAtom::new([2,3,9,99,999]),
                TestAtom::new([4,5,6])
            )); "overlapping"
        )]
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([5,6]),
            Both; "disjoint"
        )]
        fn union(a: TestAtom, b: TestAtom ,exp: AtomOperationRes<TestAtom>) {
            let res = a.union(&b);
            match (res,exp) {
                (EmptySet,EmptySet) => (),
                (Lhs,Lhs) => (),
                (Rhs,Rhs) => (),
                (Both,Both) => (),
                (Compound(One(ra)),Compound(One(ea))) => assert_eq!(ra,ea),
                (Compound(Two(ra,rb)),Compound(Two(ea,eb))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                },
                (Compound(Three(ra,rb,rc)),Compound(Three(ea,eb,ec))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                    assert_eq!(rc,ec);
                },
                (Compound(Four(ra,rb,rc,rd)),Compound(Four(ea,eb,ec,ed))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                    assert_eq!(rc,ec);
                    assert_eq!(rd,ed);
                },
                _ => panic!("Result and expected doesn't match the expected form")
            };
        }
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([1,2,3]),
            Lhs; "equal"
        )]
        #[test_case(
            TestAtom::new([1,2,3,4,5]),
            TestAtom::new([1,2,3]),
            Rhs; "superset"
        )]
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([1,2,3,50,10]),
            Lhs; "subset"
        )]
        #[test_case(
            TestAtom::new([1,2,3,10,20,30]),
            TestAtom::new([2,3,4,5,10,20,30]),
            Compound(One(
                TestAtom::new([2,3,10,20,30])
            )); "overlapping"
        )]
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([5,6]),
            EmptySet; "disjoint"
        )]
        fn intersect(a: TestAtom, b: TestAtom ,exp: AtomOperationRes<TestAtom>) {
            let res = a.intersect(&b);
            match (res,exp) {
                (EmptySet,EmptySet) => (),
                (Lhs,Lhs) => (),
                (Rhs,Rhs) => (),
                (Both,Both) => (),
                (Compound(One(ra)),Compound(One(ea))) => assert_eq!(ra,ea),
                (Compound(Two(ra,rb)),Compound(Two(ea,eb))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                },
                (Compound(Three(ra,rb,rc)),Compound(Three(ea,eb,ec))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                    assert_eq!(rc,ec);
                },
                (Compound(Four(ra,rb,rc,rd)),Compound(Four(ea,eb,ec,ed))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                    assert_eq!(rc,ec);
                    assert_eq!(rd,ed);
                },
                _ => panic!("Result and expected doesn't match the expected form")
            };
        }
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([1,2,3]),
            EmptySet; "equal"
        )]
        #[test_case(
            TestAtom::new([1,2,3,4,5]),
            TestAtom::new([1,2,3]),
            Compound(One(
                TestAtom::new([4,5])
            )); "superset"
        )]
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([1,2,3,50,10]),
            EmptySet; "subset"
        )]
        #[test_case(
            TestAtom::new([1,2,3,10,20,30]),
            TestAtom::new([2,4,5,10,20,30]),
            Compound(One(
                TestAtom::new([1,3])
            )); "overlapping"
        )]
        #[test_case(
            TestAtom::new([1,2,3]),
            TestAtom::new([5,6]),
            Lhs; "disjoint"
        )]
        fn subtract(a: TestAtom, b: TestAtom ,exp: AtomOperationRes<TestAtom>) {
            let res = a.subtract(&b);
            match (res,exp) {
                (EmptySet,EmptySet) => (),
                (Lhs,Lhs) => (),
                (Rhs,Rhs) => (),
                (Both,Both) => (),
                (Compound(One(ra)),Compound(One(ea))) => assert_eq!(ra,ea),
                (Compound(Two(ra,rb)),Compound(Two(ea,eb))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                },
                (Compound(Three(ra,rb,rc)),Compound(Three(ea,eb,ec))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                    assert_eq!(rc,ec);
                },
                (Compound(Four(ra,rb,rc,rd)),Compound(Four(ea,eb,ec,ed))) => {
                    assert_eq!(ra,ea);
                    assert_eq!(rb,eb);
                    assert_eq!(rc,ec);
                    assert_eq!(rd,ed);
                },
                _ => panic!("Result and expected doesn't match the expected form")
            };
        }

    }

    mod set_atom_ops {
        use super::*;
        use test_case::test_case;
        use std::assert_eq;
        
        #[test_case(
            DisjointAtomsSet(vec![
                TestAtom::new([1,2,3,4]),
                TestAtom::new([40,50,60]),
            ]),
            TestAtom::new([2,3,4,5,40]),
            DisjointAtomsSet(vec![
                TestAtom::new([5]),
                TestAtom::new([40]),
                TestAtom::new([50,60]),
                TestAtom::new([4,3,2]),
                TestAtom::new([1]),
            ]);"one results"
        )]
        fn union(set: TestSet, atm: TestAtom, exp: TestSet) {
            let res = set.atom_union(atm);
            assert_eq!(res.0,exp.0);
        }

    }
    
}