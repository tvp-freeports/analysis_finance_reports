use std::ops::{BitOr,BitAnd,Div};

mod indipendent_atoms;
mod ast_simple;
mod ast_smart;

#[derive(Debug)]
enum SetRelation {
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
enum SetOps{
    Union,
    Inter,
    Sub
}

trait Container {
    type Elem: ?Sized;
    fn contains(&self,e: &Self::Elem) -> bool;
}

trait Overlappable<Rhs>: {
    fn set_relation(&self,other: &Rhs) -> SetRelation;
}




trait SetAlgebra: 
BitOr<Self,Output=Self> +
BitAnd<Self,Output=Self> +
Div<Self,Output=Self> +
Sized {}

trait Set<E>:
Container<Elem=E> +
SetAlgebra 
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