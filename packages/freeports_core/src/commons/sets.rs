use std::ops::{BitOr,BitAnd,Div};

mod indipendent_atoms;
mod ast_simple;
mod ast_smart;

pub use indipendent_atoms::{DisjointAtomsSet,AtomOperations,CompoundAtomOperationRes,AtomAlgebra};
pub use ast_smart::{SmartAstSet,SmartAstNode};
pub use ast_simple::{AstSet,AstNode};


#[derive(Debug,PartialEq)]
pub enum SetRelation {
    Overlapping,
    Subset,
    Superset,
    Disjoint,
    Equal
}

impl SetOps {
    fn call(&self, a: bool, b: bool) -> bool {
        match self {
            Self::Union => a || b,
            Self::Inter => a && b,
            Self::Sub => a && !b,
        }
    }
}



#[derive(Clone,Copy,Debug,PartialEq)]
pub enum SetOps{
    Union,
    Inter,
    Sub
}

pub trait Container {
    type Elem: ?Sized;
    fn contains(&self,e: &Self::Elem) -> bool;
}

pub trait Overlappable<Rhs> {
    fn set_relation(&self,other: &Rhs) -> SetRelation;
}

pub enum Set<S,E>
where
    S: Container<Elem=E> + SetAlgebra,
    E: ?Sized
{
    Empty,
    Universe,
    Set(S)
}

impl<S,E> Container for Set<S,E>
where
    S: Container<Elem=E> + SetAlgebra,
    E: ?Sized
{
    type Elem = E;
    fn contains(&self,ele: &Self::Elem) -> bool {
        match self {
            Self::Empty => false,
            Self::Universe => true,
            Self::Set(set) => set.contains(ele)
        }
    }
}

impl<S,E> BitOr<Self> for Set<S,E>
where
    S: Container<Elem=E> + SetAlgebra,
    E: ?Sized
{
    type Output=Self;
    fn bitor(self,rhs: Self) -> Self {
        match (self,rhs) {
            (Self::Universe,_) => Self::Universe,
            (_,Self::Universe) => Self::Universe,
            (a,Self::Empty) => a,
            (Self::Empty,b) => b,
            (Self::Set(a),Self::Set(b)) => Self::Set(a | b)
        }
    }
}
impl<S,E> BitAnd<Self> for Set<S,E>
where
    S: Container<Elem=E> + SetAlgebra,
    E: ?Sized
{
    type Output=Self;
    fn bitand(self,rhs: Self) -> Self {
        match (self,rhs) {
            (a,Self::Universe) => a,
            (Self::Universe,b) => b,
            (Self::Empty,_) => Self::Empty,
            (_,Self::Empty) => Self::Empty,
            (Self::Set(a),Self::Set(b)) => Self::Set(a & b)
        }
    }
}
impl<S,E> Div<Self> for Set<S,E>
where
    S: Container<Elem=E> + SetAlgebra,
    E: ?Sized
{
    type Output=Self;
    fn div(self,rhs: Self) -> Self {
        match (self,rhs) {
            (Self::Universe,_) => todo!(),
            (_,Self::Universe) => Self::Empty,
            (a,Self::Empty) => a,
            (Self::Empty,_) => Self::Empty,
            (Self::Set(a),Self::Set(b)) => Self::Set(a / b)
        }
    }
}



pub trait SetAlgebra: 
BitOr<Self,Output=Self> +
BitAnd<Self,Output=Self> +
Div<Self,Output=Self> +
Sized {}

pub trait UncomparableSet<E>:
Container<Elem=E> +
SetAlgebra 
where E: ?Sized {}

pub trait ComparableSet<E>:
Container<Elem=E> +
SetAlgebra +
Overlappable<Self>
where E: ?Sized {}



#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use pretty_assertions::assert_eq;

    #[test_case(SetOps::Union,true,true,true)]
    #[test_case(SetOps::Union,true,false,true)]
    #[test_case(SetOps::Union,false,true,true)]
    #[test_case(SetOps::Union,false,false,false)]
    #[test_case(SetOps::Inter,true,true,true)]
    #[test_case(SetOps::Inter,true,false,false)]
    #[test_case(SetOps::Inter,false,true,false)]
    #[test_case(SetOps::Inter,false,false,false)]
    #[test_case(SetOps::Sub,true,true,false)]
    #[test_case(SetOps::Sub,true,false,true)]
    #[test_case(SetOps::Sub,false,true,false)]
    #[test_case(SetOps::Sub,false,false,false)]
    fn evaluate_setops(op: SetOps, a: bool, b: bool, res: bool){
        assert_eq!(op.call(a,b),res);
    }
}