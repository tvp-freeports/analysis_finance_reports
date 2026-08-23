//! Normalizzazione di stringhe per il confronto di nomi di fondi e societa'.
//!
//! Tre funzioni pure, di aggressivita' crescente, che servono a riconoscere come "lo stesso
//! nome" scritture che differiscono solo per accenti, punteggiatura, maiuscole o spaziatura.
//! Sono la base di [`crate::core::match_fund::MatchFund`] e, piu' avanti, del matching
//! societario di `formats_utils::text_filter`.
//!
//! Porting da `freeports_core` (`src/core/normalization.rs`, a sua volta port di
//! `_internals/core/normalization.py`): la logica e' identica, spariscono solo i wrapper
//! `#[pyfunction]` — in questo crate non c'e' confine Python (`PLAN.md` §3). Il modulo e'
//! totale: nessuna funzione puo' fallire, quindi non ha un proprio enum d'errore.

/// Aggiunge a `out` la sostituzione normalizzata di un singolo carattere gia' minuscolo.
///
/// Riproduce la tabella `str.maketrans` dell'originale Python: le lettere latine accentate
/// collassano sull'equivalente ASCII (alcune, come `ß`/`œ`/`æ`, si espandono in piu' di un
/// carattere), la punteggiatura separatrice (`,-–+`) diventa uno spazio, la punteggiatura di
/// rumore (`!?{}[]()"'’/.`) sparisce, tutto il resto passa invariato.
fn push_translated(out: &mut String, c: char) {
    match c {
        'é' | 'è' | 'ê' | 'ë' => out.push('e'),
        'á' | 'à' | 'â' | 'ä' => out.push('a'),
        'í' | 'ì' | 'î' | 'ï' => out.push('i'),
        'ó' | 'ò' | 'ô' | 'ö' => out.push('o'),
        'ú' | 'ù' | 'û' | 'ü' => out.push('u'),
        'ñ' => out.push('n'),
        'ç' => out.push('c'),
        'ß' => out.push_str("ss"),
        'å' => out.push('a'),
        'ø' => out.push('o'),
        'œ' => out.push_str("oe"),
        'æ' => out.push_str("ae"),
        '&' => out.push_str("and"),
        ',' | '-' | '–' | '+' => out.push(' '),
        '!' | '?' | '{' | '}' | '[' | ']' | '(' | ')' | '"' | '\'' | '’' | '/' | '.' => {}
        other => out.push(other),
    }
}

/// Normalizzazione profonda: minuscole, via accenti e punteggiatura di rumore, separatori
/// convertiti in spazi, sequenze di spazi collassate in uno solo.
///
/// E' la forma usata per l'*identita'* di un fondo: due nomi che normalizzano uguale sono
/// considerati lo stesso fondo.
pub fn deep_normalize_string(input: &str) -> String {
    let lowered = input.to_lowercase();
    let mut translated = String::with_capacity(lowered.len());
    for c in lowered.chars() {
        push_translated(&mut translated, c);
    }
    translated.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalizzazione leggera: trim, minuscole opzionali, sequenze di spazi collassate.
/// A differenza di [`deep_normalize_string`] non tocca accenti ne' punteggiatura.
pub fn normalize_string(input: &str, lower: bool) -> String {
    let trimmed = input.trim();
    let cased = if lower { trimmed.to_lowercase() } else { trimmed.to_string() };
    cased.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalizzazione di una singola parola: rimuove *tutti* gli spazi interni (non li collassa,
/// come fa [`normalize_string`]) e opzionalmente porta in minuscolo.
pub fn normalize_word(input: &str, lower: bool) -> String {
    let concatenated: String = input.split_whitespace().collect();
    if lower { concatenated.to_lowercase() } else { concatenated }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod deep {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("Hello World", "hello world"; "minuscole di base")]
        #[test_case("  Hello   World  ", "hello world"; "trim e collasso spazi")]
        #[test_case("Café", "cafe"; "una lettera accentata")]
        #[test_case("MÜLLER", "muller"; "accentate maiuscole abbassate prima")]
        #[test_case("Straße", "strasse"; "eszett espande in doppia s")]
        #[test_case("Rock & Roll", "rock and roll"; "e commerciale espande in and")]
        #[test_case("A,B-C–D+E", "a b c d e"; "separatori diventano spazi")]
        #[test_case("Don't say \"no\"!", "dont say no"; "rumore cancellato non spaziato")]
        #[test_case("café œuf æon", "cafe oeuf aeon"; "espansioni multi carattere")]
        #[test_case("It’s fine", "its fine"; "apostrofo tipografico cancellato")]
        #[test_case("Øresund", "oresund"; "lettere scandinave")]
        #[test_case("ÁÀÂÄ ÍÌÎÏ ÓÒÔÖ ÚÙÛÜ Ñ Ç", "aaaa iiii oooo uuuu n c"; "tutte le classi di accenti")]
        #[test_case("{a}[b](c)?d/e.f", "abcdef"; "tutta la punteggiatura di rumore")]
        #[test_case("", ""; "stringa vuota")]
        #[test_case("   ", ""; "solo spazi")]
        #[test_case("...", ""; "solo rumore")]
        #[test_case("---", ""; "solo separatori")]
        fn normalizza_come_atteso(input: &str, expected: &str) {
            assert_eq!(deep_normalize_string(input), expected);
        }

        #[test]
        fn e_idempotente() {
            for input in ["Café  Fund–A", "Rock & Roll", "Straße 1", "  ", "ØMEGA/AB"] {
                let once = deep_normalize_string(input);
                assert_eq!(deep_normalize_string(&once), once, "input: {input:?}");
            }
        }

        #[test]
        fn non_lascia_mai_spazi_doppi_o_ai_bordi() {
            for input in ["  a - b  ", "a,,,b", "a + + b", "-a-", "  Café  ,  Fund  "] {
                let out = deep_normalize_string(input);
                assert!(!out.contains("  "), "spazi doppi in {out:?} (input {input:?})");
                assert_eq!(out.trim(), out, "bordi non puliti in {out:?}");
            }
        }
    }

    mod string {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("  Hello   World  ", true, "hello world"; "minuscole di default")]
        #[test_case("  Hello   World  ", false, "Hello World"; "conserva le maiuscole")]
        #[test_case("", true, ""; "stringa vuota")]
        #[test_case("NoWhitespace", false, "NoWhitespace"; "token singolo invariato")]
        #[test_case("Café", false, "Café"; "gli accenti non vengono toccati")]
        #[test_case("a\tb\nc", false, "a b c"; "tab e newline contano come spazi")]
        #[test_case("A, B", false, "A, B"; "la punteggiatura resta")]
        fn normalizza_come_atteso(input: &str, lower: bool, expected: &str) {
            assert_eq!(normalize_string(input, lower), expected);
        }

        #[test]
        fn e_idempotente_per_entrambi_i_casing() {
            for input in ["  A  B  ", "Café Fund", ""] {
                for lower in [true, false] {
                    let once = normalize_string(input, lower);
                    assert_eq!(normalize_string(&once, lower), once, "input {input:?} lower {lower}");
                }
            }
        }
    }

    mod word {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("  Hello World  ", false, "HelloWorld"; "rimuove tutti gli spazi, tiene le maiuscole")]
        #[test_case("  Hello World  ", true, "helloworld"; "rimuove tutti gli spazi e abbassa")]
        #[test_case("Test", false, "Test"; "parola singola invariata")]
        #[test_case("Test", true, "test"; "parola singola abbassata")]
        #[test_case("", false, ""; "stringa vuota")]
        #[test_case("   ", false, ""; "solo spazi")]
        #[test_case("a\tb\nc", false, "abc"; "tab e newline rimossi come gli spazi")]
        fn normalizza_come_atteso(input: &str, lower: bool, expected: &str) {
            assert_eq!(normalize_word(input, lower), expected);
        }

        #[test]
        fn non_contiene_mai_spazi() {
            for input in ["  a b  c ", "\t\n", "x", ""] {
                assert!(!normalize_word(input, false).contains(char::is_whitespace), "input {input:?}");
            }
        }
    }

    /// I tre livelli non sono intercambiabili: questo modulo fissa in cosa differiscono, cosi'
    /// che un cambio accidentale di uno dei tre rompa un test invece di passare inosservato.
    mod differenze_fra_i_livelli {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn solo_deep_rimuove_accenti_e_punteggiatura() {
            let input = "Café, S.p.A.";
            assert_eq!(deep_normalize_string(input), "cafe spa");
            assert_eq!(normalize_string(input, true), "café, s.p.a.");
        }

        #[test]
        fn solo_word_elimina_gli_spazi_invece_di_collassarli() {
            let input = "A  B";
            assert_eq!(normalize_string(input, false), "A B");
            assert_eq!(normalize_word(input, false), "AB");
        }
    }
}
