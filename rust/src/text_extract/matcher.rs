use regex::Regex;


pub struct PdfBlockTable;



pub fn normalize_string(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = false;

    for ch in input.chars() {
        let replacement: Option<String> = match ch {
            'é' | 'è' | 'ê' | 'ë' => Some("e".into()),
            'á' | 'à' | 'â' | 'ä' => Some("a".into()),
            'í' | 'ì' | 'î' | 'ï' => Some("i".into()),
            'ó' | 'ò' | 'ô' | 'ö' => Some("o".into()),
            'ú' | 'ù' | 'û' | 'ü' => Some("u".into()),
            'ñ' => Some("n".into()),
            'ç' => Some("c".into()),
            'å' => Some("a".into()),
            'ø' => Some("o".into()),
            'œ' => Some("oe".into()),
            'æ' => Some("ae".into()),
            'ß' => Some("ss".into()),
            '&' => Some("and".into()),

            ',' | '-' | '–' | '+' => Some(" ".into()),

            '!' | '?' | '{' | '}' | '[' | ']' | '(' | ')' |
            '"' | '\'' | '’' | '/' | '.' => None,

            c if c.is_whitespace() => Some(" ".into()),
            c => Some(c.to_lowercase().to_string()),
        };

        if let Some(rep) = replacement {
            if rep == " " {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            } else {
                out.push_str(&rep);
                last_was_space = false;
            }
        }
    }

    out.trim().to_string()
}


struct Company{
    name: String,
    buds: Vec<String>,
    regexs: Vec<Regex>,
    symbols: Vec<Regex>
}


fn match_exact_name<'a>(text: &str, target_companies: &'a[Company]) -> Option<&'a str> {
    let txt=normalize_string(text);
    for c in target_companies {
        let n_name=normalize_string(&c.name);
        if txt.contains(&n_name){
            return Some(&c.name)
        }
    }
    None
}


#[cfg(test)]
mod tests {
    use test_case::test_case;
    use std::sync::LazyLock;
    use super::*;
    use pretty_assertions::{assert_eq};
    

    #[test_case("Coca Cola","coca cola"; "lower")]
    #[test_case(" \thello  i am the\n\n fox\t","hello i am the fox"; "withespaces")]
    #[test_case("áàâäéèêëíìîïóòôöúùûü","aaaaeeeeiiiioooouuuu"; "axcents")]
    #[test_case("œæß&ñçåø","oeaessandncao"; "some unusual chars")]
    #[test_case("ooo,oo-o+oooo–o","ooo oo o oooo o"; "separating chars")]
    #[test_case("a!b?c{d}e[f]g(h)i\"j'k’l/m.n","abcdefghijklmn"; "chars to remove")]
    fn string_normalization(provided: &str, expected: &str) {
        assert_eq!(
            normalize_string(provided),
            expected
        )
    }

    mod match_companies {
        use super::*;
        use pretty_assertions::{assert_eq};
        use test_case::test_case;
        const EMPTY_COMPANY: Company = Company{
            name: String::new(),
            buds: Vec::<String>::new(),
            regexs: Vec::<Regex>::new(),
            symbols: Vec::<Regex>::new(),
        };
        static COMPANY_LIST: LazyLock<Vec<Company>> = LazyLock::new(
            || vec![
                Company{
                    name: "Coca Cola".to_string(),
                    ..EMPTY_COMPANY
                },
                Company{
                    name: "BlackRock".to_string(),
                    ..EMPTY_COMPANY
                },
                Company{
                    name: "pimpa Co.".to_string(),
                    ..EMPTY_COMPANY
                },
                Company{
                    name: "almade".to_string(),
                    ..EMPTY_COMPANY
                }
            ]
        );
        
        #[test_case(" The COCA COLA company","Coca Cola";"just name")]
        fn name_contained(provided: &str, expected: &str) {
            let res = match_exact_name(provided,&COMPANY_LIST).unwrap();
            assert_eq!(
                res,expected
            )
        }
        
    }




}