use super::sets::{Container,Set,AstNode,SetOps};

pub type FontSet = Set<FontAstLeaf,str>;

#[derive(Debug,PartialEq,Clone)]
pub struct FontAstLeaf(String);

pub fn normalize_font(input: &str) -> String {
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
    out
}

impl Container for FontAstLeaf {
    type Elem = str;
    fn contains(&self, font: &str) -> bool {
        let Self(font_name) = self;
        (*font_name)==normalize_font(font)
    }
}

impl FontSet {
    pub fn new(input_txt: &str) -> Self {
        Self(AstNode::Leaf(FontAstLeaf(
            normalize_font(input_txt)
        )))
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
    fn test_normalize_font(input: &str, res: &str) {
        assert_eq!(normalize_font(input),res.to_string());
    }
    #[test]
    fn new_textset() {
        let input = "Liquor& ca/io ";
        let res = "liquorand-ca-io";
        let Set(AstNode::Leaf(FontAstLeaf(content))) = FontSet::new(input) else {
            panic!("Expected have to be a FontSet with just one leaf")
        };
        assert_eq!(content,res.to_string());
    }
    #[test]
    fn element_in_leafset() {
        let text_set="casa Sapaforica/L";
        let text="CASA,SAPAFORICA,l";
        let set = FontSet::new(text_set);
        match set {
            Set(AstNode::Leaf(leaf)) => assert!(leaf.contains(text)),
            _ => panic!("Expected have to be a TextSet with just one leaf")
        }
    }
    #[test]
    fn element_not_in_leafset() {
        let text_set="casa Semaforica/L";
        let text="CASA,SAPAFORICA,l";
        let set = FontSet::new(text_set);
        match set {
            Set(AstNode::Leaf(leaf)) => assert!(!leaf.contains(text)),
            _ => panic!("Expected have to be a TextSet with just one leaf")
        }
    }
}
