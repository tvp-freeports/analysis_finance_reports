use super::{Container,Overlappable,SetRelation,Set,SetAlgebra};
use std::ops::{BitOr, BitAnd, Div};

enum CompoundAtomOperationRes<T> {
    One(T),
    Two(T,T),
    Three(T,T,T),
    Four(T,T,T,T)
}

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
    type UnionOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes;
    fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes;
    fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes;
    fn union_overlapping(&self, other: &Self) -> Self::UnionOverlappingRes;
}

trait AtomAlgebra: Overlappable<Self> + AtomOperations {
    fn union(&self, other: &Self) -> AtomOperationRes<Self> {
        use SetRelation::*;
        use AtomOperationRes::*;
        match self.set_relation(&other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => Compound(
                self.union_overlapping(&other).into()
            ),
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
            Equal | Superset => EmptySet,
            Subset => Compound(
                self.subtract_subset(&other).into()
            ),
            Overlapping => Compound(
                self.subtract_overlapping(&other).into()
            ),
            Disjoint => Lhs,
        }
    }
}


#[derive(Clone)]
struct DisjointAtomsSet<A,E>(Vec<A>)
where
    A: AtomAlgebra + Container<Elem=E> + Clone,
    E: ?Sized
;

impl<A,E> DisjointAtomsSet<A,E>
where 
    A: AtomAlgebra + Container<Elem=E> + Clone,
    E: ?Sized
{
    fn from_atom(atom: A) -> Self {
        Self(vec![atom])
    }

    fn atom_union(mut self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut other_pieces = Vec::with_capacity(4 * self.0.len());
        other_pieces.push(other);
        for atm in &self.0 {
            let mut i = 0;
            while i < other_pieces.len() {
                match other_pieces[i].subtract(atm) {
                    EmptySet => {
                        other_pieces.remove(i);
                    },
                    Lhs => i += 1,
                    Rhs => {
                        other_pieces[i] = atm.clone();
                        i += 1;
                    },
                    Both => {
                        other_pieces.insert(i + 1, atm.clone());
                        i += 2;
                    },
                    Compound(One(a)) => {
                        other_pieces[i] = a;
                        i += 1;
                    },
                    Compound(Two(a, b)) => {
                        other_pieces[i] = a;
                        other_pieces.insert(i + 1, b);
                        i += 2;
                    },
                    Compound(Three(a, b, c)) => {
                        other_pieces[i] = a;
                        other_pieces.insert(i + 1, b);
                        other_pieces.insert(i + 2, c);
                        i += 3;
                    },
                    Compound(Four(a, b, c, d)) => {
                        other_pieces[i] = a;
                        other_pieces.insert(i + 1, b);
                        other_pieces.insert(i + 2, c);
                        other_pieces.insert(i + 3, d);
                        i += 4;
                    }
                }
            }
        }
        self.0.extend(other_pieces);
        self
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
    A: AtomAlgebra + Container<Elem=E> + Clone,
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
    A: AtomAlgebra + Container<Elem=E> + Clone,
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
    A: AtomAlgebra + Container<Elem=E> + Clone,
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
    A: AtomAlgebra + Container<Elem=E> + Clone,
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
    A: AtomAlgebra + Container<Elem = E> + Clone,
    E: ?Sized
{}

impl<A,E> Set<E> for DisjointAtomsSet<A,E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone,
    E: ?Sized
{}


#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use pretty_assertions::assert_eq;
    use std::collections::HashSet;
    
}