use crate::commons::sets::{Container,SmartAstSet,Overlappable,SetRelation};

#[derive(Debug,PartialEq,Clone)]
pub struct TextAstLeaf{
    start: bool,
    content: String,
    end: bool
}

impl Overlappable<Self> for TextAstLeaf {
    fn set_relation(&self,other: &TextAstLeaf) -> SetRelation {
        use SetRelation::*;
        let (a,b) = (&self.content,&other.content);
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
                if a==b {Equal} else if a.starts_with(b) {Subset} else if b.starts_with(a) {Superset} else {Disjoint}
            },
            (false,true,false,true) => {
                if a==b {Equal} else if a.ends_with(b) {Subset} else if b.ends_with(a) {Superset} else {Disjoint}
            },
            //----------------------------
            (false,false,false,true) => if b.contains(a) {Superset} else {Overlapping},
            (false,false,true,false) => if b.contains(a) {Superset} else {Overlapping},
            (false,true,false,false) => if a.contains(b) {Subset} else {Overlapping},
            (true,false,false,false) => if a.contains(b) {Subset} else {Overlapping},
            //----------------------------
            (false,false,false,false) => {
                if a==b {Equal} else if a.contains(b) {Subset} else if b.contains(a) {Superset} else {Overlapping}
            }
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

type TextSet = SmartAstSet<TextAstLeaf,str>;


impl TextAstLeaf {
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
        Self{
            start,
            content,
            end
        }
    }
}


impl TextSet {
    pub fn new(input_txt: &str) -> Self {
        Self::from_leaf(
            TextAstLeaf::new(input_txt)
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
        let res = TextAstLeaf::new(input);
        assert_eq!(res,expected);
    }

    #[test_case("casa","Nico si casal de lim";"contained")]
    #[test_case("^ magnone seli"," magnone seli cumas";"begin")]
    #[test_case("to tha suco$$","nunca to tha suco$";"end")]
    #[test_case("^^j$","^j";"exact")]
    fn element_in_leafset(text_set: &str, text: &str ) {
        let leaf = TextAstLeaf::new(text_set);
        assert!(leaf.contains(text));
    }

    #[test_case("casa","Nico si Casas de lim";"contained")]
    #[test_case("^ magnone seli","um magnone seli cumas";"begin")]
    #[test_case("to tha suco$$","nunca to tha suco$ demais";"end")]
    #[test_case("^^j$",".^j.";"exact")]
    fn element_not_in_leafset(text_set: &str, text: &str ) {
        let leaf = TextAstLeaf::new(text_set);
        assert!(!leaf.contains(text));
    }
    use SetRelation::*;
    #[test_case("^lemure$",Equal,"^lemure$";"equal both exact")]
    #[test_case("gremure$",Equal,"gremure$";"equal end of both vincolated")]
    #[test_case("^;leMut ",Equal,"^;leMut ";"equal start of both vincolated")]
    #[test_case(";Mut ",Equal,";Mut ";"equal both substrings")]
    #[test_case("^lemure$",Subset,"^lemu";"subset first exact second start vincolated")]
    #[test_case("^gremure$",Subset,"mure$";"subset first exact second end vincolated")]
    #[test_case("^;leMut fm",Subset,"^;leMut ";"subset start of both vincolated")]
    #[test_case(";leMut fm$",Subset,"Mut fm$";"subset end of both vincolated")]
    #[test_case("^;Mut $",Subset,"Mu";"subset first exact second substring")]
    #[test_case("^;Mu",Subset,"Mu";"subset first start vincolated second substring")]
    #[test_case("ut $",Subset,"u";"subset first end vincolated second substring")]
    #[test_case(" nisp o y-utusv",Subset,"o y-utu";"subset both substrings")]
    #[test_case("^l emure",Superset,"^l emureti cos(8)$";"superset first start vincolated second exact")]
    #[test_case("mure][$",Superset,"^gremure][$";"superset first end vincolated second exact")]
    #[test_case("^;leM",Superset,"^;leMut ";"superset start of both vincolated")]
    #[test_case(" fm$",Superset,"Mut fm$";"superset end of both vincolated")]
    #[test_case("t ",Superset,"^;Mut $";"superset first substring second exact")]
    #[test_case("Mu",Superset,"^;Mut";"superset first substring second start vincolated")]
    #[test_case("kut",Superset,"makut $";"superset first substring second end vincolated")]
    #[test_case("tutu",Superset,"malitutu";"superset both substrings")]
    #[test_case("^l emure",Overlapping,"cos(8)$";"overlapping first start second end vincolated")]
    #[test_case("mure][$",Overlapping,"^gre";"overlapping first end second start vincolated")]
    #[test_case("^;leM",Overlapping,"giummo";"overlapping first start vincolated second substring")]
    #[test_case(" fm$",Overlapping,"giummo";"overlapping first end vincolated second substring")]
    #[test_case("dribbo",Overlapping,"^;Mut obbo";"overlapping first substring second start vincolated")]
    #[test_case("dribbo",Overlapping,";Mut fibbo$";"overlapping first sbustring second end vincolated")]
    #[test_case("canimo",Overlapping,";::::;";"overlapping both substrings")]
    #[test_case("^l emure$",Disjoint,"^cos(8)$";"disjoint both exact")]
    #[test_case("^;leM",Disjoint,"^giummo";"disjoint both start vincolated")]
    #[test_case(" fm$",Disjoint,"giummo$";"disjoint both end vincolated")]
    #[test_case("^mure][$",Disjoint,"^gre";"disjoint exact second start vincolated")]
    #[test_case("^mure][$",Disjoint,"gre$";"disjoint exact second end vincolated")]
    #[test_case("^dribbo",Disjoint,"^;Mut obbo$";"disjoint first start vincolated second exact")]
    #[test_case("dribbo$",Disjoint,"^;Mut obbo$";"disjoint first end vincolated second exact")]
    fn set_relation(a: &str, rel: SetRelation, b: &str) {
        assert_eq!(
            TextAstLeaf::new(a).set_relation(
                &TextAstLeaf::new(b)
            ),
            rel
        )
    }


}

