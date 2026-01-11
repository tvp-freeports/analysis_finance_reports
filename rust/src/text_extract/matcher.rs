


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