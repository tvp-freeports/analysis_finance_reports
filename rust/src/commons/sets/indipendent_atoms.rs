use super::{Container,Overlappable,SetRelation,Set,SetAlgebra};
use std::fmt::Debug;
use std::ops::{BitOr, BitAnd, Div};
use std::collections::HashSet;
use std::hash::Hash;

#[derive(Debug)]
pub enum CompoundAtomOperationRes<T> {
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


pub trait AtomOperations: Sized + Clone {
    type SubtractSubsetRes: Into<CompoundAtomOperationRes<Self>>;
    type SubtractOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    type IntersectOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes;
    fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes;
    fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes;
    fn union_overlapping(&self, other: &Self) -> CompoundAtomOperationRes<Self> {
        use CompoundAtomOperationRes::*;
        match self.subtract_overlapping(other).into() {
            One(a) => Two(a,(*other).clone()),
            Two(a,b) => Three(a,b,(*other).clone()),
            Three(a,b,c) => Four(a,b,c,(*other).clone()),
            _ => unreachable!("Default implementation of union doesn't support that set subtraction")
        }
    }

}

pub trait AtomAlgebra: Overlappable<Self> + AtomOperations {
    fn union(&self, other: &Self) -> AtomOperationRes<Self> {
        use SetRelation::*;
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        match self.set_relation(&other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => Compound(self.union_overlapping(other)),
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
pub struct DisjointAtomsSet<A,E>(HashSet<A>)
where
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug + Eq + Hash,
    E: ?Sized
;





impl<A,E> DisjointAtomsSet<A,E>
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug + Eq + Hash,
    E: ?Sized
{
    pub fn atoms_ref(&self) -> HashSet<&A> {
        let mut atoms = HashSet::new();
        for a in self.0.iter() {
            atoms.insert(a);
        }
        atoms
    }
    pub fn from_atom(atom: A) -> Self {
        let mut atoms = HashSet::new();
        atoms.insert(atom);
        Self(atoms)
    }
    fn atom_union(&self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut new_set = HashSet::new();
        for atm in &self.0 {
            match atm.subtract(&other) {
                EmptySet => (),
                Lhs => {
                    new_set.insert(atm.clone());
                },
                Compound(One(a)) => {
                    new_set.insert(a);
                },
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_union"),
            };
            
        }
        new_set.insert(other);
        Self(new_set)
    }

    fn atom_intersection(&self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut atoms: Vec<A> = self.0.iter().map(|a| a.clone()).collect();
        let mut i = 0;
        while i < atoms.len() {
            match atoms[i].intersect(&other) {
                EmptySet => {
                    atoms.remove(i);
                },
                Lhs => i += 1,
                Rhs => {
                    atoms[i] = other.clone();
                    i += 1;
                },
                Compound(One(a)) => {
                    atoms[i] = a;
                    i += 1;
                },
                Compound(Two(a, b)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    i += 2;
                },
                Compound(Three(a, b, c)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    i += 3;
                },
                Compound(Four(a, b, c, d)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    atoms.insert(i + 3, d);
                    i += 4;
                },
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_intersection"),
            }
        }
        let mut s=HashSet::with_capacity(atoms.len());
        for a in atoms {
            s.insert(a);
        }
        Self(s)
    }

    fn atom_subtraction(&self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut atoms: Vec<A> = self.0.iter().map(|a| a.clone()).collect();
        let mut i = 0;
        while i < atoms.len() {
            match atoms[i].subtract(&other) {
                EmptySet => {
                    atoms.remove(i);
                },
                Lhs => i += 1,
                Compound(One(a)) => {
                    atoms[i] = a;
                    i += 1;
                },
                Compound(Two(a, b)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    i += 2;
                },
                Compound(Three(a, b, c)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    i += 3;
                },
                Compound(Four(a, b, c, d)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    atoms.insert(i + 3, d);
                    i += 4;
                },
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_subtraction"),
            }
        }
        let mut s=HashSet::with_capacity(atoms.len());
        for a in atoms {
            s.insert(a);
        }
        Self(s)
    }
    fn union(&self,other: &Self) -> Self {
        let mut res = Self(self.0.clone());
        for o in &other.0 {
            res=res.atom_union(o.clone());
        }
        res
    }
    fn intersect(&self,other: &Self) -> Self {
        let mut res = Self(HashSet::new());
        for o in &other.0 {
            res=res | self.atom_intersection(o.clone());
        }
        res
    }
    fn subtract(&self,other: &Self) -> Self {
        let mut res = Self(self.0.clone());
        for o in &other.0 {
            res=res.atom_subtraction(o.clone());
        }
        res
    }
}

impl<A,E> BitOr<Self> for DisjointAtomsSet<A,E> 
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug + Eq + Hash,
    E: ?Sized
{
    type Output = Self;
    fn bitor(self,other: Self) -> Self {
        self.union(&other)
    }
}
impl<A,E> BitAnd<Self> for DisjointAtomsSet<A,E> 
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug + Eq + Hash,
    E: ?Sized
{
    type Output = Self;
    fn bitand(self,other: Self) -> Self {
        self.intersect(&other)
    }
}
impl<A,E> Div<Self> for DisjointAtomsSet<A,E> 
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug + Eq + Hash,
    E: ?Sized
{
    type Output = Self;
    fn div(self,other: Self) -> Self {
        self.subtract(&other)
    }
}

impl<A,E> Container for DisjointAtomsSet<A,E>
where 
    A: AtomAlgebra + Container<Elem=E> + Clone + Debug + Eq + Hash,
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
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized
{}


impl<A, E> Overlappable<Self> for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    fn set_relation(&self, other: &Self) -> SetRelation {
        use SetRelation::*;
        let mut disjoint = true;
        'outer: for set_atom in &self.0 {
            for o in &other.0 {
                if set_atom.set_relation(o) != Disjoint {
                    disjoint = false;
                    break 'outer
                }
            }
        }
        if disjoint {
            Disjoint
        } else {
            let self_is_contained = self.subtract(&other).0 == HashSet::new();
            let other_is_contained = other.subtract(&self).0 == HashSet::new();
            if other_is_contained && self_is_contained {
                Equal
            } else if other_is_contained {
                Superset
            } else if self_is_contained {
                Subset
            } else {
                Overlapping
            }
        }
    }
}

impl<A,E> Set<E> for DisjointAtomsSet<A,E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized
{}




#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use std::collections::BTreeSet;
    #[derive(Clone,Debug,PartialEq,Eq,Hash)]
    struct TestAtom(BTreeSet<u32>);
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
        One(BTreeSet<u32>),
        Two(BTreeSet<u32>,BTreeSet<u32>),
        Three(BTreeSet<u32>,BTreeSet<u32>,BTreeSet<u32>),
        Four(BTreeSet<u32>,BTreeSet<u32>,BTreeSet<u32>,BTreeSet<u32>)
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
            let res_set: BTreeSet<&u32>=self.0.intersection(&other.0).collect();
            if res_set.len() == 2 {
                let mut i=res_set.into_iter();
                Two(
                    BTreeSet::from([*i.next().unwrap()]),
                    BTreeSet::from([*i.next().unwrap()])
                )
            } else if res_set.len() == 4 {
                let mut i=res_set.into_iter();
                Four(
                    BTreeSet::from([*i.next().unwrap()]),
                    BTreeSet::from([*i.next().unwrap()]),
                    BTreeSet::from([*i.next().unwrap()]),
                    BTreeSet::from([*i.next().unwrap()])
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
            Self(BTreeSet::from(vec))
        }
    }
    impl TestSet {
        fn new<const N: usize>(vec: [u32; N]) -> Self {
            Self::from_atom(TestAtom::new(vec))
        }
    }
    #[test]
    fn from_atom() {
        let a=TestSet::from_atom(TestAtom(BTreeSet::from([20,30,40])));
        assert_eq!(a.0,HashSet::from([TestAtom(BTreeSet::from([20,30,40]))]));
    }
    #[test]
    fn atoms_ref() {
        let a = TestAtom(BTreeSet::from([20,30,40]));
        let b = TestAtom(BTreeSet::from([80,60,20]));
        let c = TestAtom(BTreeSet::from([81,61,21]));
        let mut set = HashSet::new();
        let mut set_ref = HashSet::new();
        set_ref.insert(&a);
        set_ref.insert(&b);
        set_ref.insert(&c);
        set.insert(a.clone());
        set.insert(b.clone());
        set.insert(c.clone());
        let res = DisjointAtomsSet(set);
        assert_eq!(res.atoms_ref(),set_ref);
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
            Compound(Two(
                TestAtom::new([1,10]),
                TestAtom::new([4,5,6,2,3,9,99,999]),
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
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([1,2,3,4]),
                TestAtom::new([40,50,60]),
            ])),
            TestAtom::new([2,3,4,5,40]),
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([50,60]),
                TestAtom::new([1]),
                TestAtom::new([40,5,4,3,2]),
            ]));"simple"
        )]
        fn union(set: TestSet, atm: TestAtom, exp: TestSet) {
            let res = set.atom_union(atm);
            assert_eq!(res.0,exp.0);
        }

        #[test_case(
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([1,3,4,5,6,7,8]),
                TestAtom::new([2,9,10,11,12]),
                TestAtom::new([13,14,20]),
                TestAtom::new([30,50]),
            ])),
            TestAtom::new([3,4,5,6,7,8,9,10,11,12,13,14,30,50]),
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([3,4,5,6,7,8]),
                TestAtom::new([9]),
                TestAtom::new([10]),
                TestAtom::new([11]),
                TestAtom::new([12]),
                TestAtom::new([13]),
                TestAtom::new([14]),
                TestAtom::new([30,50]),
            ]));"simple"
        )]
        fn intersect(set: TestSet, atm: TestAtom, exp: TestSet) {
            let res = set.atom_intersection(atm);
            assert_eq!(res.0,exp.0);
        }

        #[test_case(
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([1,3,4,5,6,7,8]),
                TestAtom::new([2,9,10,11,12]),
                TestAtom::new([13,14,20]),
                TestAtom::new([30,50]),
            ])),
            TestAtom::new([1,2,9,10,11,12,13]),
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([3,4,5,6,7,8]),
                TestAtom::new([14,20]),
                TestAtom::new([30,50]),
            ]));"simple"
        )]
        fn subtraction(set: TestSet, atm: TestAtom, exp: TestSet) {
            let res = set.atom_subtraction(atm);
            assert_eq!(res.0,exp.0);
        }

    }


    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10]),
            TestAtom::new([13,14]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([10,2]),
            TestAtom::new([9]),
            TestAtom::new([13,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,14]),
            TestAtom::new([9]),
            TestAtom::new([2,10])
        ]));"equal"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
                TestAtom::new([2,9,10,11,12]),
                TestAtom::new([13,14,20]),
            ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([1,3,4,5,6,7,8]),
            TestAtom::new([30,50]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([1,3,4,5,6,7,8]),
            TestAtom::new([2,9,10,11,12]),
            TestAtom::new([13,14,20]),
            TestAtom::new([30,50]),
        ]));"disjoint"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10,11,12]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,13,20]),
            TestAtom::new([11,12,14]),
            TestAtom::new([9,10])
        ]));"superset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([9]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,13,20]),
            TestAtom::new([11,12,14]),
            TestAtom::new([9])
        ]));"subset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,99]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([9,34]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,13,20]),
            TestAtom::new([11,12,14]),
            TestAtom::new([9,34]),
             TestAtom::new([99])
        ]));"overlapping"
    )]
    fn union(a: TestSet, b: TestSet, exp: TestSet) {
        let c = a | b;
        assert_eq!(c.0,exp.0);
    }


    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10]),
            TestAtom::new([13,14]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([10,2]),
            TestAtom::new([9]),
            TestAtom::new([13,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,14]),
            TestAtom::new([9]),
            TestAtom::new([2,10])
        ]));"equal"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
                TestAtom::new([2,9,10,11,12]),
                TestAtom::new([13,14,20]),
            ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([1,3,4,5,6,7,8]),
            TestAtom::new([30,50]),
        ])),
        DisjointAtomsSet(HashSet::new());"disjoint"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10,11,12]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2]),
            TestAtom::new([13]),
            TestAtom::new([11]),
            TestAtom::new([12]),
            TestAtom::new([20]),
            TestAtom::new([14]),
        ]));"superset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([9]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2]),
            TestAtom::new([13]),
            TestAtom::new([20]),
            TestAtom::new([14]),
            TestAtom::new([9])
        ]));"subset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,99]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([9,34]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2]),
            TestAtom::new([9]),
            TestAtom::new([20]),
            TestAtom::new([13]),
            TestAtom::new([14])
        ]));"overlapping"
    )]
    fn intersect(a: TestSet, b: TestSet, exp: TestSet) {
        let c = a & b;
        assert_eq!(c.0,exp.0);
    }

    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10]),
            TestAtom::new([13,14]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([10,2]),
            TestAtom::new([9]),
            TestAtom::new([13,14]),

        ])),
        DisjointAtomsSet(HashSet::new());"equal"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10,11,12]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([1,3,4,5,6,7,8]),
            TestAtom::new([30,50]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10,11,12]),
            TestAtom::new([13,14,20]),
        ]));"disjoint"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,10,11,12]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([9,10])
        ]));"superset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([13,2,20]),
            TestAtom::new([9]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::new());"subset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,9,99]),
            TestAtom::new([13,14,20]),
        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([2,20]),
            TestAtom::new([9,34]),
            TestAtom::new([11,12,14]),

        ])),
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([99]),
            TestAtom::new([13])
        ]));"overlapping"
    )]
    fn subtract(a: TestSet, b: TestSet, exp: TestSet) {
        let c = a / b;
        assert_eq!(c.0,exp.0);
    }

    #[test]
    fn expression() {
        let a = DisjointAtomsSet::from_atom(TestAtom::new([1,2,3,4]));
        let b = DisjointAtomsSet::from_atom(TestAtom::new([0,2,3,4]));
        let c = DisjointAtomsSet::from_atom(TestAtom::new([0,5,3,40]));
        let d = DisjointAtomsSet::from_atom(TestAtom::new([1,2]));
        let e = DisjointAtomsSet::from_atom(TestAtom::new([1,20]));
        let f = a & ( b / c ) | e;
        assert_eq!(f.0,HashSet::from([TestAtom::new([1,20]),TestAtom::new([2,4])]));
    }

    #[test_case(
        TestSet::new([4,5,70]),
        70;
        "simple"
    )]
    #[test_case(
        TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]),
        60;
        "union"
    )]
    #[test_case(
        TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]),
        60;
        "intersect"
    )]
    #[test_case(
        TestSet::new([2,4,6,8]) / TestSet::new([4,6]),
        8;
        "subtraction"
    )]
    #[test_case(
        TestSet::new([6]) | (TestSet::new([3,89]) & TestSet::new([56,67,89]) / TestSet::new([67,78])),
        89;
        "expression"
    )]
    fn element_in_set(test_set: TestSet, n: u32){
        assert!(test_set.contains(&n));
    }
    #[test_case(
        TestSet::new([4,5,70]),
        71;
        "simple"
    )]
    #[test_case(
        TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]),
        603;
        "union"
    )]
    #[test_case(
        TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]),
        80;
        "intersect"
    )]
    #[test_case(
        TestSet::new([2,4,6,8]) / TestSet::new([4,6]),
        4;
        "subtraction"
    )]
    #[test_case(
        TestSet::new([6]) | (TestSet::new([3,89,67]) & TestSet::new([56,67,89]) / TestSet::new([67,78])),
        67;
        "expression"
    )]
    fn element_not_in_set(test_set: TestSet, n: u32){
        assert!(!test_set.contains(&n));
    }
    

    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,4,5]),
            TestAtom::new([7,8,9])
        ])),
        SetRelation::Equal,
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,8]),
            TestAtom::new([5]),
            TestAtom::new([7,4,9])
        ])); "equal"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,4,5,6,0]),
            TestAtom::new([7,8,9])
        ])),
        SetRelation::Superset,
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,8]),
            TestAtom::new([5]),
            TestAtom::new([7,4,9])
        ])); "superset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,4,5]),
            TestAtom::new([7,8,9])
        ])),
        SetRelation::Subset,
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,8]),
            TestAtom::new([5]),
            TestAtom::new([500]),
            TestAtom::new([7,4,9])
        ])); "subset"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,4,5]),
            TestAtom::new([7,8])
        ])),
        SetRelation::Disjoint,
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([55,66,77]),
            TestAtom::new([9])
        ])); "disjoint"
    )]
    #[test_case(
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,4,5]),
            TestAtom::new([7,8,9,10])
        ])),
        SetRelation::Overlapping,
        DisjointAtomsSet(HashSet::from([
            TestAtom::new([3,8,5,7,4,9,41,44])
        ])); "overlapped"
    )]
    fn set_relation(a: TestSet, res: SetRelation, b: TestSet) {
        assert_eq!(a.set_relation(&b),res);
    }
}