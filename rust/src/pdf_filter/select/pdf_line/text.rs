use crate::commons::sets::{Container,Set,AstNode,SetOps,Overlappable,SetRelation};

pub type TextSet = Set<TextAstLeaf,str>;

#[derive(Debug,PartialEq,Clone)]
pub struct TextAstLeaf{
    start: bool,
    content: String,
    end: bool
}

impl Overlappable for TextAstLeaf {
    fn set_relation(&self,other: TextAstLeaf) -> bool {
        use SetRelation::*;
        let (a,b) = &(self.content,other.content);
        match (self.start,self.end,other.start,other.end) {
            (true,true,true,true) => if a == b {Equal} else {Disjoint},
            //----------------------------
            (true,true,true,false) => if a.starts_with(b) {Subset} else {Disjoint} ,
            (true,true,false,true) => if a.ends_with(b) {Subset} else {Disjoint},
            (true,false,true,true) => if b.starts_with(a) {Superset} else {Disjoint},
            (false,true,true,true) => if b.ends_with(a) {Superset} else {Disjoint},
            //----------------------------
            (true,true,false,false) => if a.contains(b) {Subset} else {Disjoint},
            (false,false,true,true) => if b.contains(a) {Superset} else {Disjoint},
            //----------------------------
            (true,false,false,true) => Overlapping,
            (false,true,true,false) => Overlapping,
            //----------------------------
            (true,false,true,false) => {
                if a==b {Equal} else if a.starts_with(b) {Subset} else if b.starts_with(a) {Subset} else {Disjoint}
            },
            (false,true,false,true) => {
                if a==b {Equal} else if a.ends_with(b) {Subset} else if b.ends_with(a) {Subset} else {Disjoint}
            },
            //----------------------------
            (false,false,false,true) => if b.contains(a) {Superset} else {Overlapping},
            (false,false,true,false) => if b.contains(a) {Superset} else {Overlapping},
            (false,true,false,false) => if a.contains(b) {Subset} else {Overlapping},
            (true,false,false,false) => if a.contains(b) {Subset} else {Overlapping},
            //----------------------------
            (false,false,false,false) => if a==b {Equal} else {Overlapping}
        }
    }
}

impl Container for TextAstLeaf {
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



impl TextSet {
    pub fn new(input_txt: &str) -> Self {
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
        Self(AstNode::Leaf(TextAstLeaf{
            start,
            content,
            end
        })) 
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
            Set(AstNode::Leaf(res)) => assert_eq!(res,expected),
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
            Set(AstNode::Leaf(leaf)) => assert!(leaf.contains(text)),
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
            Set(AstNode::Leaf(leaf)) => assert!(!leaf.contains(text)),
            _ => panic!("Expected have to be a TextSet with just one leaf")
        }
    }
}