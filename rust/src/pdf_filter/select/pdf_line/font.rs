use crate::commons::sets::{
    DisjointAtomsSet,
    Container,
    Overlappable,
    AtomOperations,
    AtomAlgebra,
    SetRelation,
    CompoundAtomOperationRes
};

#[derive(Debug,PartialEq,Clone,Hash,Eq)]
pub struct Font(String);

impl Font {
    fn new(input: &str) -> Self {
        let trimmed_input = input.trim();
        let mut out = String::with_capacity(trimmed_input.len());
        let mut last_was_div = false;
        for ch in trimmed_input.chars() {
            let replacement: Option<String> = match ch {
                'é' | 'è' | 'ê' | 'ë' => Some("e".into()),
                'á' | 'à' | 'â' | 'ä' => Some("a".into()),
                'í' | 'ì' | 'î' | 'ï' => Some("i".into()),
                'ó' | 'ò' | 'ô' | 'ö' => Some("o".into()),
                'ú' | 'ù' | 'û' | 'ü' => Some("u".into()),
                '&' => Some("and".into()),
                '{' | '(' => Some('['.into()),
                '}' | ')' => Some(']'.into()),
                ',' | '–'  | '/' | '.' => Some('-'.into()),
                c if c.is_whitespace() => Some('-'.into()),
                c => Some(c.to_lowercase().to_string()),
            };

            if let Some(rep) = replacement {
                if rep == "-" {
                    if !last_was_div {
                        out.push('-');
                        last_was_div = true;
                    }
                } else {
                    out.push_str(&rep);
                    last_was_div = false;
                }
            }
        }
        Self(out)
    }
}

impl Container for Font {
    type Elem = Font;
    fn contains(&self,other: &Self) -> bool{
        self.0==other.0
    }
}

impl Overlappable<Self> for Font {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        if self.0==other.0 {Equal} else {Disjoint}
    }
}

impl AtomOperations for Font {
    type SubtractOverlappingRes = CompoundAtomOperationRes<Font>;
    type SubtractSubsetRes = CompoundAtomOperationRes<Font>;
    type IntersectOverlappingRes = CompoundAtomOperationRes<Font>;
    fn subtract_subset(&self,_other: &Self) -> CompoundAtomOperationRes<Font> {
        unreachable!("Font cannot be a subset of another")
    }
    fn subtract_overlapping(&self,_other: &Self) -> CompoundAtomOperationRes<Font> {
        unreachable!("Font cannot be overlapping another")
    }
    fn intersect_overlapping(&self,_other: &Self) -> CompoundAtomOperationRes<Font> {
        unreachable!("Font cannot be overlapping another")
    }
}

impl AtomAlgebra for Font {}

pub type FontSet = DisjointAtomsSet<Font,Font>;

impl FontSet {
    pub fn new(font: &str) -> Self {
        Self::from_atom(Font::new(font))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;
    #[test_case("NicaRAguA","nicaragua";"lower")]
    #[test_case("ulma turman\t \n gerico\tsum","ulma-turman-gerico-sum";"withespaces")]
    #[test_case("áàâäéèêëíìîïóòôöúùûü","aaaaeeeeiiiioooouuuu"; "axcents")]
    #[test_case("oba{pes}li(cu)[b]","oba[pes]li[cu][b]"; "parenthesis")]
    #[test_case("&","and"; "some unusual chars")]
    #[test_case("ooo,oooo–o/ooo.oo","ooo-oooo-o-ooo-oo"; "separating chars")]
    #[test_case("\t \n gattopardo \n\n","gattopardo"; "trim")]
    fn new_font(input: &str, res: &str) {
        assert_eq!(Font::new(input).0,res.to_string());
    }
    // #[test_case("NicaRAguA","nicaragua";"lower")]
    // #[test_case("ulma turman\t \n gerico\tsum","ulma-turman-gerico-sum";"withespaces")]
    // #[test_case("áàâäéèêëíìîïóòôöúùûü","aaaaeeeeiiiioooouuuu"; "axcents")]
    // #[test_case("oba{pes}li(cu)[b]","oba[pes]li[cu][b]"; "parenthesis")]
    // #[test_case("&","and"; "some unusual chars")]
    // #[test_case("ooo,oooo–o/ooo.oo","ooo-oooo-o-ooo-oo"; "separating chars")]
    // #[test_case("\t \n gattopardo \n\n","gattopardo"; "trim")]
    // fn new_fontset(input: &str, res: &str) {
    //     assert_eq!(FontSet::new(input).0,res.to_string());
    // }

    #[test]
    fn element_in_atom() {
        let font_set=Font::new("casa Sapaforica/L");
        let font=Font::new("CASA,SAPAFORICA,l");
        assert!(font_set.contains(&font));
    }
    #[test]
    fn element_not_in_atom() {
        let font_set=Font::new("Liquor& ca/io ");
        let font=Font::new("CASA,Semaforica");
        assert!(!font_set.contains(&font));
    }

    #[test_case("\tcalimo",SetRelation::Equal," CalImo ";"equal")]
    #[test_case("\tcalimo",SetRelation::Disjoint," Calo ";"disjoint")]
    fn set_relation(a: &str, rel: SetRelation , b: &str) {
        assert_eq!(Font::new(a).set_relation(
            &Font::new(b)
        ),rel);
    }

    #[test]
    #[should_panic(expected="internal error: entered unreachable code: Font cannot be overlapping another")]
    fn subtract_overlapping() {
        let a=Font::new("A");
        let b=Font::new("B");
        a.subtract_overlapping(&b);
    }
    #[test]
    #[should_panic(expected="internal error: entered unreachable code: Font cannot be overlapping another")]
    fn intersect_overlapping() {
        let a=Font::new("A");
        let b=Font::new("B");
        a.intersect_overlapping(&b);
    }
    #[test]
    #[should_panic(expected="internal error: entered unreachable code: Font cannot be a subset of another")]
    fn subtract_subset() {
        let a=Font::new("A");
        let b=Font::new("B");
        a.subtract_subset(&b);
    }
}



