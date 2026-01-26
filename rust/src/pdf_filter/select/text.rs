use std::ops::{BitOr,BitAnd,Div};


trait Set {
    type Elem: ?Sized;
    fn contains(&self,e: &Self::Elem) -> bool;
}

#[derive(Debug,PartialEq,Clone)]
struct TextAstLeaf{
    start: bool,
    content: String,
    end: bool
}

impl Set for TextAstLeaf {
    type Elem = str;
    fn contains(&self, text: &str) -> bool {
        let Self{
            start,
            content,
            end
        } = self;
        if *start && *end {
            text == content
        } else if *start {
            text.starts_with(content)
        } else if *end {
            text.ends_with(content)
        } else {
            text.contains(content)
        }
    }
}


pub const ALL_TEXT: TextSet = TextSet(
    TextAstNode::Leaf(
        TextAstLeaf{
            start: false,
            content: String::new(),
            end: false
        }
    )
);

pub const NO_TEXT: TextSet = TextSet(
    TextAstNode::Leaf(
        TextAstLeaf{
            start: true,
            content: String::new(),
            end: true
        }
    )
);

#[derive(Debug,Clone)]
enum TextAstNode{
    Leaf(TextAstLeaf),
    Branch(Box<TextAstNode>, SetOps, Box<TextAstNode>)
}

#[derive(Clone,Copy,Debug,PartialEq)]
pub enum SetOps{
    Union,
    Inter,
    Sub
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

#[derive(Debug,Clone)]
pub struct TextSet(TextAstNode);

impl TextSet {
    fn new(input_txt: &str) -> Self {
        let mut content = input_txt.to_string();
        let mut start = false;
        let mut end = false;
        if input_txt.starts_with(r"\^") {
            content.remove(0);
        } else if input_txt.starts_with("^") {
            start = true;
            content.remove(0);
        }

        if input_txt.ends_with(r"\$") {
            content.remove(content.len()-2);
        } else if input_txt.ends_with("$") {
            content.pop();
            end = true;
        }
        Self(TextAstNode::Leaf(TextAstLeaf{
            start,
            content,
            end
        })) 
    }
}

impl BitOr<Self> for TextSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(
            TextAstNode::Branch(
                Box::new(self.0),
                SetOps::Union,
                Box::new(rhs.0)
            )
        )
    }
}
impl BitAnd<Self> for TextSet {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(
            TextAstNode::Branch(
                Box::new(self.0),
                SetOps::Inter,
                Box::new(rhs.0)
            )
        )
    }
}
impl Div<Self> for TextSet {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(
            TextAstNode::Branch(
                Box::new(self.0),
                SetOps::Sub,
                Box::new(rhs.0)
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;
    #[test_case("cave canem",TextAstLeaf{
        start:false,
        content: "cave canem".to_string(),
        end: false
    };"contained")]
    #[test_case(r"\^nija Glu$m  ",TextAstLeaf{
        start:false,
        content: "^nija Glu$m  ".to_string(),
        end: false
    };"contained begin escaped")]
    #[test_case(r"yogurt\$",TextAstLeaf{
        start:false,
        content: "yogurt$".to_string(),
        end: false
    };"contained end escaped")]
    #[test_case(r"\^^\^guspo$\$",TextAstLeaf{
        start:false,
        content: r"^^\^guspo$$".to_string(),
        end: false
    };"contained begin end escaped")]
    #[test_case("^ ganico ",TextAstLeaf{
        start:true,
        content: " ganico ".to_string(),
        end: false
    };"prefix")]
    #[test_case(r"lemmo$",TextAstLeaf{
        start:false,
        content: "lemmo".to_string(),
        end: true
    };"postifix")]
    #[test_case(r"^\^O$$",TextAstLeaf{
        start:true,
        content: r"\^O$".to_string(),
        end: true
    };"exact")]
    fn new_textset(input: &str, expected: TextAstLeaf) {
        match TextSet::new(input) {
            TextSet(TextAstNode::Leaf(res)) => assert_eq!(res,expected),
            _ => panic!("Expected have to be a TextSet with just one leaf")
        }
    }

    #[test_case("casa","Nico si casal de lim";"contained")]
    #[test_case("^ magnone seli"," magnone seli cumas";"begin")]
    #[test_case("to tha suco$$","nunca to tha suco$";"end")]
    #[test_case("^^j$","^j";"exact")]
    fn element_in_leafset(text_set: &str, text: &str ) {
        let set = TextSet::new(text_set);
        match set {
            TextSet(TextAstNode::Leaf(leaf)) => assert!(leaf.contains(text)),
            _ => panic!("Expected have to be a TextSet with just one leaf")
        }
    }

    #[test_case("casa","Nico si Casas de lim";"contained")]
    #[test_case("^ magnone seli","um magnone seli cumas";"begin")]
    #[test_case("to tha suco$$","nunca to tha suco$ demais";"end")]
    #[test_case("^^j$",".^j.";"exact")]
    fn element_not_in_leafset(text_set: &str, text: &str ) {
        let set = TextSet::new(text_set);
        match set {
            TextSet(TextAstNode::Leaf(leaf)) => assert!(!leaf.contains(text)),
            _ => panic!("Expected have to be a TextSet with just one leaf")
        }
    }

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


    mod expressions {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test]
        fn union() {
            let a = TextSet::new("cave");
            let TextSet(TextAstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TextSet::new("canem");
            let TextSet(TextAstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a | b;
            match c {
                TextSet(TextAstNode::Branch(
                    box_x,
                    op,
                    box_y
                )) => {
                    let TextAstNode::Leaf(x) = *box_x else {
                        panic!("unexpected node structure")
                    };
                    let TextAstNode::Leaf(y) = *box_y else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op,SetOps::Union);
                    assert_eq!(x,a_leaf);
                    assert_eq!(y,b_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            }
        }
        #[test]
        fn intersection() {
            let a = TextSet::new("cave");
            let TextSet(TextAstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TextSet::new("canem");
            let TextSet(TextAstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a & b;
            match c {
                TextSet(TextAstNode::Branch(
                    box_x,
                    op,
                    box_y
                )) => {
                    let TextAstNode::Leaf(x) = *box_x else {
                        panic!("unexpected node structure")
                    };
                    let TextAstNode::Leaf(y) = *box_y else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op,SetOps::Inter);
                    assert_eq!(x,a_leaf);
                    assert_eq!(y,b_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            }
        }
        #[test]
        fn subtraction() {
            let a = TextSet::new("cave");
            let TextSet(TextAstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TextSet::new("canem");
            let TextSet(TextAstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a / b;
            match c {
                TextSet(TextAstNode::Branch(
                    box_x,
                    op,
                    box_y
                )) => {
                    let TextAstNode::Leaf(x) = *box_x else {
                        panic!("unexpected node structure")
                    };
                    let TextAstNode::Leaf(y) = *box_y else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op,SetOps::Sub);
                    assert_eq!(x,a_leaf);
                    assert_eq!(y,b_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            } 
        }
        #[test]
        fn with_precedence() {
            let a = TextSet::new("A");
            let TextSet(TextAstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TextSet::new("B");
            let TextSet(TextAstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = TextSet::new("C");
            let TextSet(TextAstNode::Leaf(c_leaf)) = c.clone() else {
                panic!("unexpected set structure")
            };
            let d = TextSet::new("D");
            let TextSet(TextAstNode::Leaf(d_leaf)) = d.clone() else {
                panic!("unexpected set structure")
            };
            let e = TextSet::new("E");
            let TextSet(TextAstNode::Leaf(e_leaf)) = e.clone() else {
                panic!("unexpected set structure")
            };
            let f = TextSet::new("F");
            let TextSet(TextAstNode::Leaf(f_leaf)) = f.clone() else {
                panic!("unexpected set structure")
            };
            let g = a | (b / (c | d)) & (e / f);
            match g {
                TextSet(TextAstNode::Branch(
                    box_x0,
                    op0,
                    box_y0
                )) => {
                    assert_eq!(op0,SetOps::Union);
                    let TextAstNode::Leaf(should_a) = *box_x0 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_a,a_leaf);
                    let TextAstNode::Branch(box_x1,op1,box_y1) = *box_y0 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op1,SetOps::Inter);

                    let TextAstNode::Branch(box_x2,op2,box_y2) = *box_x1 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op2,SetOps::Sub);
                    let TextAstNode::Branch(box_x3,op3,box_y3) = *box_y1 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op3,SetOps::Sub);

                    let TextAstNode::Leaf(should_e) = *box_x3 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_e,e_leaf);
                    let TextAstNode::Leaf(should_f) = *box_y3 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_f,f_leaf);

                    let TextAstNode::Leaf(should_b) = *box_x2 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_b,b_leaf);
                    let TextAstNode::Branch(box_x4,op4,box_y4) = *box_y2 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op4,SetOps::Union);

                    let TextAstNode::Leaf(should_c) = *box_x4 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_c,c_leaf);
                    let TextAstNode::Leaf(should_d) = *box_y4 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_d,d_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            } 
        }
    }
}