//! Bitwise flag-expression parser/evaluator: turns strings like `"A | B & ~C"` into a
//! combined bitmask, given a name -> bit lookup table.
//!
//! Rust port of the AST-walking core of
//! `packages/freeports_core/src/freeports/_internals/commons/enum_utils.py::flag_from_string`
//! (the `_from_ast` inner function). The original used Python's `ast.parse(expr, mode="eval")`
//! plus a hand-written AST walker restricted to `BitAnd`/`BitOr`/`BitXor`/`Invert`/`Name` nodes;
//! this is a purpose-built recursive-descent parser for exactly that same small expression
//! language, so it doesn't need to invoke Python's `ast` module at all.
//!
//! **Only this expression-evaluation kernel is ported** — `enum_utils.py`'s surrounding
//! functions (`flag_to_string`, `_cast_input_flags`, `_cast_input_enum`, `input_flags`,
//! `input_enum`) stay in Python. They are generic over an arbitrary caller-supplied
//! `Type[Flag]`/`Type[Enum]` and build Pydantic `Annotated[...]` types dynamically — that is
//! Python-specific type-level plumbing with no Rust equivalent worth building; porting it would
//! mean reimplementing a slice of Python's typing/Pydantic machinery for no behavioral gain. See
//! `analysis_finance_reports/agent-memory/rust-rewrite-plan.md` for the reasoning.
//!
//! Precedence (matching Python's own bitwise operator precedence, lowest to highest):
//! `|` then `^` then `&` then unary `~`. Parentheses are supported for grouping, even though the
//! current Python callers don't appear to use them in practice — the original could accept them
//! (`ast.parse` parses arbitrary Python expressions), so this does too, for fidelity.

use std::collections::HashMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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

fn tokenize(expr: &str) -> Result<Vec<Token>, String> {
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
            other => return Err(format!("Unexpected character {other:?} in flag expression")),
        }
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    names: &'a HashMap<String, u64>,
    /// OR of every valid flag's bit value — the "universe" `~` inverts within. Matches Python
    /// `Flag.__invert__`, which masks the complement to the flag class's own defined bits
    /// instead of doing an open-ended infinite-precision (or fixed-width) integer complement:
    /// `~F.A` on a 3-member flag class is `F.B|F.C`, not some huge out-of-range value.
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
    fn parse_or(&mut self) -> Result<u64, String> {
        let mut value = self.parse_xor()?;
        while matches!(self.peek(), Some(Token::Pipe)) {
            self.advance();
            value |= self.parse_xor()?;
        }
        Ok(value)
    }

    // xor_expr ::= and_expr ( '^' and_expr )*
    fn parse_xor(&mut self) -> Result<u64, String> {
        let mut value = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Caret)) {
            self.advance();
            value ^= self.parse_and()?;
        }
        Ok(value)
    }

    // and_expr ::= unary ( '&' unary )*
    fn parse_and(&mut self) -> Result<u64, String> {
        let mut value = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::Amp)) {
            self.advance();
            value &= self.parse_unary()?;
        }
        Ok(value)
    }

    // unary ::= '~' unary | primary
    fn parse_unary(&mut self) -> Result<u64, String> {
        if matches!(self.peek(), Some(Token::Tilde)) {
            self.advance();
            let inner = self.parse_unary()?;
            return Ok(!inner & self.universe);
        }
        self.parse_primary()
    }

    // primary ::= NAME | '(' or_expr ')'
    fn parse_primary(&mut self) -> Result<u64, String> {
        match self.advance() {
            Some(Token::Name(name)) => {
                let upper = name.to_uppercase();
                self.names
                    .get(&upper)
                    .copied()
                    .ok_or_else(|| format!("Invalid flag {upper}"))
            }
            Some(Token::LParen) => {
                let value = self.parse_or()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(value),
                    _ => Err("Expected closing parenthesis".to_string()),
                }
            }
            other => Err(format!("Unexpected token in flag expression: {other:?}")),
        }
    }
}

/// Parses and evaluates a bitwise flag expression (e.g. `"A | B & ~C"`) against a name -> bit
/// lookup table, returning the combined bitmask. Names are matched case-insensitively
/// (uppercased before lookup, matching the Python original).
pub fn evaluate(expression: &str, names: &HashMap<String, u64>) -> Result<u64, String> {
    let tokens = tokenize(expression)?;
    if tokens.is_empty() {
        return Err("Empty flag expression".to_string());
    }
    let universe = names.values().fold(0u64, |acc, b| acc | b);
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        names,
        universe,
    };
    let value = parser.parse_or()?;
    if parser.pos != tokens.len() {
        return Err("Unexpected trailing tokens in flag expression".to_string());
    }
    Ok(value)
}

/// Python-visible wrapper: evaluate a flag expression against a `{name: bit}` dict, returning
/// the combined bitmask. Raises `ValueError` on an unknown flag name or malformed expression
/// (the Python original lets a malformed expression propagate as a `SyntaxError` from
/// `ast.parse` instead — a deliberately accepted, purely cosmetic difference in exception type
/// for the malformed-syntax case; unknown-flag-name already raised `ValueError` in both).
#[pyfunction]
#[pyo3(name = "evaluate_flag_expression")]
pub fn py_evaluate_flag_expression(expression: &str, names: HashMap<String, u64>) -> PyResult<u64> {
    evaluate(expression, &names).map_err(PyValueError::new_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> HashMap<String, u64> {
        HashMap::from([
            ("A".to_string(), 0b001),
            ("B".to_string(), 0b010),
            ("C".to_string(), 0b100),
        ])
    }

    #[test]
    fn single_name() {
        assert_eq!(evaluate("A", &names()), Ok(0b001));
    }

    #[test]
    fn or_combines_bits() {
        assert_eq!(evaluate("A | B", &names()), Ok(0b011));
    }

    #[test]
    fn and_intersects_bits() {
        assert_eq!(evaluate("A | B & B", &names()), Ok(0b011));
    }

    #[test]
    fn xor_toggles_bits() {
        assert_eq!(evaluate("(A | B) ^ B", &names()), Ok(0b001));
    }

    #[test]
    fn unary_not_inverts_within_the_flag_universe() {
        // Matches Python's `Flag.__invert__`: ~A on a {A, B, C} flag class is B|C, not an
        // open-ended bit complement — verified empirically against `enum.Flag` before porting
        // (`~F.A` on a 3-member Flag class gives `.value == 6`, i.e. B|C, not a huge number).
        assert_eq!(evaluate("~A", &names()), Ok(0b110));
    }

    #[test]
    fn case_insensitive_names() {
        assert_eq!(evaluate("a | b", &names()), Ok(0b011));
    }

    #[test]
    fn parentheses_change_precedence() {
        // Without parens, `&` binds tighter than `|`, so `A | B & C` == `A | (B & C)`.
        assert_eq!(evaluate("A | B & C", &names()), evaluate("A | (B & C)", &names()));
        // With parens forcing the `|` first, the result differs when C's bit isn't in A|B.
        let with_parens = evaluate("(A | B) & C", &names()).unwrap();
        let without_parens = evaluate("A | B & C", &names()).unwrap();
        assert_ne!(with_parens, without_parens);
    }

    #[test]
    fn double_negation_is_identity_within_the_universe() {
        assert_eq!(evaluate("~~A", &names()), Ok(0b001));
    }

    #[test]
    fn invert_of_union_leaves_nothing_in_the_universe() {
        assert_eq!(evaluate("~(A | B | C)", &names()), Ok(0));
    }

    #[test]
    fn unknown_flag_name_is_an_error() {
        assert!(evaluate("D", &names()).is_err());
    }

    #[test]
    fn malformed_expression_is_an_error() {
        assert!(evaluate("A |", &names()).is_err());
        assert!(evaluate("A B", &names()).is_err());
        assert!(evaluate("(A", &names()).is_err());
    }

    #[test]
    fn empty_expression_is_an_error() {
        assert!(evaluate("", &names()).is_err());
    }
}
