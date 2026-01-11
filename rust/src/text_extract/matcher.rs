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



#[cfg(test)]
mod tests {
    use test_case::test_case;
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


}