//! A bitwise flag-expression parser and evaluator: turns strings like `"A | B & ~C"` into a
//! combined bitmask, given a name-to-bit lookup table.
//!
//! It exists so that a formats repository can write an algorithm's flags as a readable expression
//! in a configuration cell, instead of a magic integer.
//!
//! # Semantics
//!
//! - precedence, lowest to highest: `|`, then `^`, then `&`, then unary `~`, left to right within
//!   a level;
//! - parentheses group and override precedence, and nest arbitrarily;
//! - names are matched case-insensitively, uppercased **before** lookup. The lookup table's own
//!   keys are not normalised, so a table keyed by lowercase names will not match;
//! - unary `~` complements against the *universe* — the OR of every bit present in the lookup
//!   table — not against a fixed width. `~A` over `{A, B, C}` is `B | C`, never some huge
//!   out-of-range value.
//!
//! Every failure has its own error variant rather than a single message string, so a caller can
//! tell an unknown flag name from a syntax error and report accordingly.

use std::collections::HashMap;

use thiserror::Error;

/// Failure modes of [`evaluate`] — see the module doc above for the exact contract each variant
/// covers.
#[derive(Debug, Error)]
pub enum FlagExprError {
    #[error("unknown flag {name}")]
    UnknownFlag { name: String },
    #[error("unexpected character {character:?} in flag expression")]
    UnexpectedCharacter { character: char },
    #[error("empty flag expression")]
    EmptyExpression,
    #[error("unexpected token in flag expression")]
    UnexpectedToken,
    #[error("unclosed parenthesis in flag expression")]
    UnclosedParenthesis,
    #[error("trailing tokens in flag expression")]
    TrailingTokens,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Pipe,
    Caret,
    Amp,
    Tilde,
    LParen,
    RParen,
}

fn tokenize(expr: &str) -> Result<Vec<Token>, FlagExprError> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '|' => {
                chars.next();
                tokens.push(Token::Pipe);
            }
            '^' => {
                chars.next();
                tokens.push(Token::Caret);
            }
            '&' => {
                chars.next();
                tokens.push(Token::Amp);
            }
            '~' => {
                chars.next();
                tokens.push(Token::Tilde);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Name(name));
            }
            other => return Err(FlagExprError::UnexpectedCharacter { character: other }),
        }
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    names: &'a HashMap<String, u64>,
    /// OR of every valid flag's bit value — the "universe" `~` inverts within. Matches Python
    /// `Flag.__invert__`, which masks the complement to the flag class's own defined bits instead
    /// of doing an open-ended infinite-precision (or fixed-width) integer complement: `~F.A` on a
    /// 3-member flag class is `F.B|F.C`, not some huge out-of-range value.
    universe: u64,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos);
        self.pos += 1;
        t
    }

    // or_expr ::= xor_expr ( '|' xor_expr )*
    fn parse_or(&mut self) -> Result<u64, FlagExprError> {
        let mut value = self.parse_xor()?;
        while matches!(self.peek(), Some(Token::Pipe)) {
            self.advance();
            value |= self.parse_xor()?;
        }
        Ok(value)
    }

    // xor_expr ::= and_expr ( '^' and_expr )*
    fn parse_xor(&mut self) -> Result<u64, FlagExprError> {
        let mut value = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Caret)) {
            self.advance();
            value ^= self.parse_and()?;
        }
        Ok(value)
    }

    // and_expr ::= unary ( '&' unary )*
    fn parse_and(&mut self) -> Result<u64, FlagExprError> {
        let mut value = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::Amp)) {
            self.advance();
            value &= self.parse_unary()?;
        }
        Ok(value)
    }

    // unary ::= '~' unary | primary
    fn parse_unary(&mut self) -> Result<u64, FlagExprError> {
        if matches!(self.peek(), Some(Token::Tilde)) {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(!inner & self.universe);
        }
        self.parse_primary()
    }

    // primary ::= NAME | '(' or_expr ')'
    fn parse_primary(&mut self) -> Result<u64, FlagExprError> {
        match self.advance() {
            Some(Token::Name(name)) => {
                let upper = name.to_uppercase();
                self.names
                    .get(&upper)
                    .copied()
                    .ok_or(FlagExprError::UnknownFlag { name: upper })
            }
            Some(Token::LParen) => {
                let value = self.parse_or()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(value),
                    _ => Err(FlagExprError::UnclosedParenthesis),
                }
            }
            _ => Err(FlagExprError::UnexpectedToken),
        }
    }
}

/// Parses and evaluates a bitwise flag expression (e.g. `"A | B & ~C"`) against a name -> bit
/// lookup table, returning the combined bitmask. Names are matched case-insensitively
/// (uppercased before lookup).
pub fn evaluate(expression: &str, names: &HashMap<String, u64>) -> Result<u64, FlagExprError> {
    let tokens = tokenize(expression)?;
    if tokens.is_empty() {
        return Err(FlagExprError::EmptyExpression);
    }
    let universe = names.values().fold(0u64, |acc, b| acc | b);
    let mut parser = Parser { tokens: &tokens, pos: 0, names, universe };
    let value = parser.parse_or()?;
    if parser.pos != tokens.len() {
        return Err(FlagExprError::TrailingTokens);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Three independent, non-overlapping bits — good enough for most tests. Tests that need to
    /// see precedence actually change the *result* (not just re-derive it via parens on both
    /// sides) build their own small table with overlapping bits instead.
    fn names() -> HashMap<String, u64> {
        HashMap::from([
            ("A".to_string(), 0b001u64),
            ("B".to_string(), 0b010u64),
            ("C".to_string(), 0b100u64),
        ])
    }

    mod single_name {
        use super::*;

        #[test]
        fn resolves_to_its_bit() {
            assert_eq!(evaluate("A", &names()).unwrap(), 0b001);
        }
    }

    mod or_operator {
        use super::*;

        #[test]
        fn combines_the_bits_of_both_operands() {
            assert_eq!(evaluate("A | B", &names()).unwrap(), 0b011);
        }

        #[test]
        fn chains_left_to_right_across_more_than_two_operands() {
            assert_eq!(evaluate("A | B | C", &names()).unwrap(), 0b111);
        }
    }

    mod and_operator {
        use super::*;

        #[test]
        fn intersects_the_bits_of_both_operands() {
            assert_eq!(evaluate("(A | B) & B", &names()).unwrap(), 0b010);
        }

        #[test]
        fn binds_tighter_than_xor() {
            // Overlapping bits chosen so that grouping actually changes the result: with
            // A=0b001, B=0b011, C=0b010, `A ^ B & C` must parse as `A ^ (B & C)`, not
            // `(A ^ B) & C` (those two give different values with this table).
            let names = HashMap::from([
                ("A".to_string(), 0b001u64),
                ("B".to_string(), 0b011u64),
                ("C".to_string(), 0b010u64),
            ]);
            assert_eq!(
                evaluate("A ^ B & C", &names).unwrap(),
                evaluate("A ^ (B & C)", &names).unwrap()
            );
            assert_ne!(
                evaluate("A ^ B & C", &names).unwrap(),
                evaluate("(A ^ B) & C", &names).unwrap()
            );
        }
    }

    mod xor_operator {
        use super::*;

        #[test]
        fn toggles_bits_present_in_exactly_one_operand() {
            assert_eq!(evaluate("(A | B) ^ B", &names()).unwrap(), 0b001);
        }

        #[test]
        fn binds_tighter_than_or() {
            // A=0b010, B=0b001, C=0b011: `A | B ^ C` must parse as `A | (B ^ C)`, not
            // `(A | B) ^ C` — those two differ with this table.
            let names = HashMap::from([
                ("A".to_string(), 0b010u64),
                ("B".to_string(), 0b001u64),
                ("C".to_string(), 0b011u64),
            ]);
            assert_eq!(
                evaluate("A | B ^ C", &names).unwrap(),
                evaluate("A | (B ^ C)", &names).unwrap()
            );
            assert_ne!(
                evaluate("A | B ^ C", &names).unwrap(),
                evaluate("(A | B) ^ C", &names).unwrap()
            );
        }
    }

    mod unary_not {
        use super::*;

        #[test]
        fn inverts_within_the_flag_universe_not_open_endedly() {
            // Matches Python's `Flag.__invert__`: ~A on a {A, B, C} flag class is B|C, not an
            // open-ended bit complement (verified empirically against `enum.Flag` before
            // porting: `~F.A` on a 3-member Flag class has `.value == 6`, i.e. B|C).
            assert_eq!(evaluate("~A", &names()).unwrap(), 0b110);
        }

        #[test]
        fn double_negation_is_identity() {
            assert_eq!(evaluate("~~A", &names()).unwrap(), 0b001);
            assert_eq!(
                evaluate("~~(A | B)", &names()).unwrap(),
                evaluate("A | B", &names()).unwrap()
            );
        }

        #[test]
        fn binds_tighter_than_every_binary_operator() {
            // `~A | B` must be `(~A) | B`, not `~(A | B)`.
            assert_eq!(
                evaluate("~A | B", &names()).unwrap(),
                evaluate("(~A) | B", &names()).unwrap()
            );
            assert_ne!(
                evaluate("~A | B", &names()).unwrap(),
                evaluate("~(A | B)", &names()).unwrap()
            );
            // `~A & B` must be `(~A) & B`, not `~(A & B)`.
            assert_eq!(
                evaluate("~A & B", &names()).unwrap(),
                evaluate("(~A) & B", &names()).unwrap()
            );
            assert_ne!(
                evaluate("~A & B", &names()).unwrap(),
                evaluate("~(A & B)", &names()).unwrap()
            );
        }

        #[test]
        fn inverting_the_full_universe_leaves_nothing() {
            assert_eq!(evaluate("~(A | B | C)", &names()).unwrap(), 0);
        }

        #[test]
        fn inverting_a_zero_value_yields_the_full_universe() {
            assert_eq!(evaluate("~(A ^ A)", &names()).unwrap(), 0b111);
        }

        #[test]
        fn universe_only_includes_bits_the_lookup_table_actually_defines() {
            // C is deliberately absent from this table: `~A`'s universe must be A|B only, not
            // some wider fixed-width complement that happens to include C's bit pattern too.
            let names = HashMap::from([("A".to_string(), 0b001u64), ("B".to_string(), 0b010u64)]);
            assert_eq!(evaluate("~A", &names).unwrap(), 0b010);
        }
    }

    mod parentheses {
        use super::*;

        #[test]
        fn override_default_precedence() {
            assert_eq!(evaluate("A | B & C", &names()).unwrap(), 0b001);
            assert_eq!(evaluate("(A | B) & C", &names()).unwrap(), 0b000);
        }

        #[test]
        fn nest_arbitrarily() {
            assert_eq!(evaluate("((A | B) & (B | C))", &names()).unwrap(), 0b010);
        }
    }

    mod case_insensitivity {
        use super::*;

        #[test]
        fn lowercase_input_resolves_the_same_bit_as_uppercase() {
            assert_eq!(evaluate("a", &names()).unwrap(), evaluate("A", &names()).unwrap());
        }

        #[test]
        fn mixed_case_input_resolves() {
            assert_eq!(evaluate("a | B", &names()).unwrap(), 0b011);
        }

        #[test]
        fn unknown_flag_error_reports_the_uppercased_name_even_for_lowercase_input() {
            match evaluate("d", &names()) {
                Err(FlagExprError::UnknownFlag { name }) => assert_eq!(name, "D"),
                other => panic!("expected UnknownFlag, got {other:?}"),
            }
        }

        #[test]
        fn lookup_table_keys_must_already_be_uppercase_for_matching_to_work() {
            // The query name is uppercased before lookup, but the table's own keys are used
            // as-is: a table keyed by lowercase names does not match, even though the "same"
            // name (case-insensitively) is present. This is the reference's actual behaviour,
            // not a bug being introduced here.
            let names = HashMap::from([("a".to_string(), 1u64)]);
            match evaluate("A", &names) {
                Err(FlagExprError::UnknownFlag { name }) => assert_eq!(name, "A"),
                other => panic!("expected UnknownFlag, got {other:?}"),
            }
        }
    }

    mod whitespace {
        use super::*;

        #[test]
        fn is_optional_around_operators() {
            assert_eq!(evaluate("A|B", &names()).unwrap(), evaluate("A | B", &names()).unwrap());
        }

        #[test]
        fn extra_whitespace_is_ignored() {
            assert_eq!(
                evaluate("   A   |   B   ", &names()).unwrap(),
                evaluate("A | B", &names()).unwrap()
            );
        }
    }

    mod name_syntax {
        use super::*;

        #[test]
        fn names_may_contain_digits_after_the_first_character() {
            let names = HashMap::from([("A1".to_string(), 0b1u64)]);
            assert_eq!(evaluate("A1", &names).unwrap(), 0b1);
        }

        #[test]
        fn names_may_contain_underscores() {
            let names = HashMap::from([("FLAG_ONE".to_string(), 0b1u64)]);
            assert_eq!(evaluate("FLAG_ONE", &names).unwrap(), 0b1);
        }
    }

    mod error_cases {
        use super::*;

        mod unknown_flag_name {
            use super::*;

            #[test]
            fn reports_the_offending_name() {
                match evaluate("D", &names()) {
                    Err(FlagExprError::UnknownFlag { name }) => assert_eq!(name, "D"),
                    other => panic!("expected UnknownFlag, got {other:?}"),
                }
            }

            #[test]
            fn triggers_even_with_an_empty_lookup_table() {
                match evaluate("A", &HashMap::new()) {
                    Err(FlagExprError::UnknownFlag { name }) => assert_eq!(name, "A"),
                    other => panic!("expected UnknownFlag, got {other:?}"),
                }
            }
        }

        mod invalid_characters {
            use super::*;

            #[test]
            fn rejects_arithmetic_operators() {
                match evaluate("A + B", &names()) {
                    Err(FlagExprError::UnexpectedCharacter { character }) => {
                        assert_eq!(character, '+');
                    }
                    other => panic!("expected UnexpectedCharacter, got {other:?}"),
                }
            }

            #[test]
            fn rejects_a_name_starting_with_a_digit() {
                match evaluate("1A", &names()) {
                    Err(FlagExprError::UnexpectedCharacter { character }) => {
                        assert_eq!(character, '1');
                    }
                    other => panic!("expected UnexpectedCharacter, got {other:?}"),
                }
            }

            #[test]
            fn rejects_punctuation() {
                assert!(matches!(
                    evaluate("A!", &names()),
                    Err(FlagExprError::UnexpectedCharacter { .. })
                ));
            }
        }

        mod empty_expression {
            use super::*;

            #[test]
            fn rejects_the_empty_string() {
                assert!(matches!(evaluate("", &names()), Err(FlagExprError::EmptyExpression)));
            }

            #[test]
            fn rejects_a_whitespace_only_string() {
                assert!(matches!(evaluate("   ", &names()), Err(FlagExprError::EmptyExpression)));
            }
        }

        mod unexpected_token {
            use super::*;

            #[test]
            fn rejects_a_trailing_operator_with_no_right_operand() {
                assert!(matches!(evaluate("A |", &names()), Err(FlagExprError::UnexpectedToken)));
            }

            #[test]
            fn rejects_a_leading_operator_with_no_left_operand() {
                assert!(matches!(evaluate("| A", &names()), Err(FlagExprError::UnexpectedToken)));
            }

            #[test]
            fn rejects_an_empty_parenthesized_group() {
                assert!(matches!(evaluate("()", &names()), Err(FlagExprError::UnexpectedToken)));
            }

            #[test]
            fn rejects_a_lone_unary_operator() {
                assert!(matches!(evaluate("~", &names()), Err(FlagExprError::UnexpectedToken)));
            }
        }

        mod unclosed_parenthesis {
            use super::*;

            #[test]
            fn rejects_a_group_with_no_closing_paren() {
                assert!(matches!(evaluate("(A", &names()), Err(FlagExprError::UnclosedParenthesis)));
            }

            #[test]
            fn rejects_a_group_followed_by_an_unexpected_token_instead_of_the_closing_paren() {
                // Inside the parens, `A` is already a complete sub-expression by the time `B` is
                // reached (there's no operator joining them) — so the parser is looking for `)`
                // and finds `B` instead. This is a closing-paren failure, not a trailing-tokens
                // one, because it happens *inside* the still-open group.
                assert!(matches!(evaluate("(A B)", &names()), Err(FlagExprError::UnclosedParenthesis)));
            }
        }

        mod trailing_tokens {
            use super::*;

            #[test]
            fn rejects_a_second_name_with_no_operator_between() {
                assert!(matches!(evaluate("A B", &names()), Err(FlagExprError::TrailingTokens)));
            }

            #[test]
            fn rejects_a_dangling_closing_paren() {
                assert!(matches!(evaluate("A)", &names()), Err(FlagExprError::TrailingTokens)));
            }

            #[test]
            fn rejects_garbage_after_a_parenthesized_group() {
                assert!(matches!(
                    evaluate("(A | B) C", &names()),
                    Err(FlagExprError::TrailingTokens)
                ));
            }
        }
    }

    /// Stress tests: the interaction of precedence, parentheses and the universe-masking of `~`
    /// is combinatorial, so it is checked over generated input against invariants — De Morgan's
    /// laws among them — rather than case by case.
    ///
    /// No random-number dependency is taken for this. `Xorshift64` below is a tiny,
    /// deterministic, fixed-seed generator, which is enough to produce a variety of expression
    /// shapes and keeps every failure reproducible.
    mod stress {
        use super::*;

        struct Xorshift64(u64);

        impl Xorshift64 {
            fn next(&mut self) -> u64 {
                let mut x = self.0;
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                self.0 = x;
                x
            }
        }

        const ALPHABET: [(&str, u64); 4] =
            [("A", 0b0001), ("B", 0b0010), ("C", 0b0100), ("D", 0b1000)];

        fn alphabet_names() -> HashMap<String, u64> {
            ALPHABET.iter().map(|(n, b)| (n.to_string(), *b)).collect()
        }

        fn universe() -> u64 {
            ALPHABET.iter().fold(0, |acc, (_, b)| acc | b)
        }

        /// Builds a random syntactically valid expression over `ALPHABET`, together with a
        /// ground-truth value computed independently of `evaluate` (plain `u64` bitwise ops on
        /// the same table), so the checks below aren't just comparing `evaluate` against itself.
        #[allow(clippy::manual_is_multiple_of)] // `% 3 == 0` reads more plainly than the lint's suggestion here.
        fn random_expr(rng: &mut Xorshift64, depth: u32) -> (String, u64) {
            if depth == 0 || rng.next() % 3 == 0 {
                let idx = (rng.next() as usize) % ALPHABET.len();
                let (name, bit) = ALPHABET[idx];
                return (name.to_string(), bit);
            }
            let (left_s, left_v) = random_expr(rng, depth - 1);
            let (right_s, right_v) = random_expr(rng, depth - 1);
            match rng.next() % 4 {
                0 => (format!("({left_s} | {right_s})"), left_v | right_v),
                1 => (format!("({left_s} & {right_s})"), left_v & right_v),
                2 => (format!("({left_s} ^ {right_s})"), left_v ^ right_v),
                _ => (format!("(~{left_s})"), !left_v & universe()),
            }
        }

        #[test]
        fn matches_an_independently_computed_ground_truth_for_many_random_expressions() {
            let names = alphabet_names();
            let mut rng = Xorshift64(0x9E37_79B9_7F4A_7C15); // fixed seed: reproducible failures
            for _ in 0..500 {
                let (expr, expected) = random_expr(&mut rng, 4);
                assert_eq!(evaluate(&expr, &names).unwrap(), expected, "expr = {expr}");
            }
        }

        #[test]
        fn double_negation_is_identity_for_many_random_expressions() {
            let names = alphabet_names();
            let mut rng = Xorshift64(0xDEAD_BEEF_CAFE_F00D);
            for _ in 0..200 {
                let (expr, value) = random_expr(&mut rng, 3);
                let negated_twice = format!("~~({expr})");
                assert_eq!(evaluate(&negated_twice, &names).unwrap(), value, "expr = {expr}");
            }
        }

        #[test]
        fn de_morgan_not_of_or_equals_and_of_nots() {
            let names = alphabet_names();
            let mut rng = Xorshift64(0x1234_5678_9ABC_DEF0);
            for _ in 0..200 {
                let (left, _) = random_expr(&mut rng, 2);
                let (right, _) = random_expr(&mut rng, 2);
                let lhs = evaluate(&format!("~({left} | {right})"), &names).unwrap();
                let rhs = evaluate(&format!("(~{left}) & (~{right})"), &names).unwrap();
                assert_eq!(lhs, rhs, "left = {left}, right = {right}");
            }
        }

        #[test]
        fn de_morgan_not_of_and_equals_or_of_nots() {
            let names = alphabet_names();
            let mut rng = Xorshift64(0x0FED_CBA9_8765_4321);
            for _ in 0..200 {
                let (left, _) = random_expr(&mut rng, 2);
                let (right, _) = random_expr(&mut rng, 2);
                let lhs = evaluate(&format!("~({left} & {right})"), &names).unwrap();
                let rhs = evaluate(&format!("(~{left}) | (~{right})"), &names).unwrap();
                assert_eq!(lhs, rhs, "left = {left}, right = {right}");
            }
        }

        #[test]
        fn or_is_idempotent_for_many_random_expressions() {
            let names = alphabet_names();
            let mut rng = Xorshift64(0x0BAD_C0DE_F00D_BABE);
            for _ in 0..200 {
                let (expr, value) = random_expr(&mut rng, 3);
                assert_eq!(
                    evaluate(&format!("({expr}) | ({expr})"), &names).unwrap(),
                    value,
                    "expr = {expr}"
                );
            }
        }

        #[test]
        fn and_is_idempotent_for_many_random_expressions() {
            let names = alphabet_names();
            let mut rng = Xorshift64(0xBABE_F00D_0BAD_C0DE);
            for _ in 0..200 {
                let (expr, value) = random_expr(&mut rng, 3);
                assert_eq!(
                    evaluate(&format!("({expr}) & ({expr})"), &names).unwrap(),
                    value,
                    "expr = {expr}"
                );
            }
        }

        #[test]
        fn or_of_two_named_flags_matches_the_union_of_their_individual_evaluations() {
            let names = alphabet_names();
            for (name_a, bit_a) in ALPHABET {
                for (name_b, bit_b) in ALPHABET {
                    let combined = evaluate(&format!("{name_a} | {name_b}"), &names).unwrap();
                    assert_eq!(combined, bit_a | bit_b);
                    let separately =
                        evaluate(name_a, &names).unwrap() | evaluate(name_b, &names).unwrap();
                    assert_eq!(combined, separately);
                }
            }
        }

        #[test]
        #[allow(clippy::type_complexity)] // local table of (name, fn) pairs, not worth a named type.
        fn exhaustive_pairs_of_names_and_operators_match_bitwise_ground_truth() {
            let names = alphabet_names();
            let ops: [(&str, fn(u64, u64) -> u64); 3] =
                [("|", |x, y| x | y), ("&", |x, y| x & y), ("^", |x, y| x ^ y)];
            for (name_a, bit_a) in ALPHABET {
                for (name_b, bit_b) in ALPHABET {
                    for (op, f) in ops {
                        let expr = format!("{name_a} {op} {name_b}");
                        assert_eq!(evaluate(&expr, &names).unwrap(), f(bit_a, bit_b), "expr = {expr}");
                    }
                }
            }
        }

        #[test]
        fn inverting_the_full_universe_always_leaves_nothing() {
            let names = alphabet_names();
            let full = ALPHABET.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(" | ");
            assert_eq!(evaluate(&format!("~({full})"), &names).unwrap(), 0);
        }
    }
}
