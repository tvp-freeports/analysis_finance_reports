//! Selecting lines by their text.
//!
//! Plain string matching with `^` and `$` anchors, not regular expressions: `^abc` is a prefix,
//! `abc$` a suffix, `^abc$` an exact match, and a bare `abc` a substring. A literal anchor is
//! escaped with a backslash.
//!
//! Anchors rather than regexes because the algebra needs more than a yes/no answer: to combine
//! selections it must decide whether one is a subset of, overlaps, or is disjoint from another, and
//! that question is answerable for these four shapes and undecidable in general for regular
//! expressions.

use crate::commons::sets::ast_smart::SmartAstSet;
use crate::commons::sets::{Container, Overlappable, SetRelation};

#[derive(Debug, PartialEq, Clone)]
pub struct TextAstLeaf {
    start: bool,
    content: String,
    end: bool,
}

impl Overlappable<Self> for TextAstLeaf {
    fn set_relation(&self, other: &TextAstLeaf) -> SetRelation {
        use SetRelation::*;
        let (a, b) = (&self.content, &other.content);
        match (self.start, self.end, other.start, other.end) {
            (true, true, true, true) => {
                if a == b {
                    Equal
                } else {
                    Disjoint
                }
            }
            (true, true, true, false) => {
                if a.starts_with(b) {
                    Subset
                } else {
                    Disjoint
                }
            }
            (true, true, false, true) => {
                if a.ends_with(b) {
                    Subset
                } else {
                    Disjoint
                }
            }
            (true, false, true, true) => {
                if b.starts_with(a) {
                    Superset
                } else {
                    Disjoint
                }
            }
            (false, true, true, true) => {
                if b.ends_with(a) {
                    Superset
                } else {
                    Disjoint
                }
            }
            (true, true, false, false) => {
                if a.contains(b) {
                    Subset
                } else {
                    Disjoint
                }
            }
            (false, false, true, true) => {
                if b.contains(a) {
                    Superset
                } else {
                    Disjoint
                }
            }
            (true, false, false, true) => Overlapping,
            (false, true, true, false) => Overlapping,
            (true, false, true, false) => {
                if a == b {
                    Equal
                } else if a.starts_with(b) {
                    Subset
                } else if b.starts_with(a) {
                    Superset
                } else {
                    Disjoint
                }
            }
            (false, true, false, true) => {
                if a == b {
                    Equal
                } else if a.ends_with(b) {
                    Subset
                } else if b.ends_with(a) {
                    Superset
                } else {
                    Disjoint
                }
            }
            (false, false, false, true) => {
                if b.contains(a) {
                    Superset
                } else {
                    Overlapping
                }
            }
            (false, false, true, false) => {
                if b.contains(a) {
                    Superset
                } else {
                    Overlapping
                }
            }
            (false, true, false, false) => {
                if a.contains(b) {
                    Subset
                } else {
                    Overlapping
                }
            }
            (true, false, false, false) => {
                if a.contains(b) {
                    Subset
                } else {
                    Overlapping
                }
            }
            (false, false, false, false) => {
                if a == b {
                    Equal
                } else if a.contains(b) {
                    Subset
                } else if b.contains(a) {
                    Superset
                } else {
                    Overlapping
                }
            }
        }
    }
}

impl Container for TextAstLeaf {
    type Elem = str;
    fn contains(&self, text: &str) -> bool {
        let Self { start, content, end } = self;
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

pub type TextSet = SmartAstSet<TextAstLeaf, str>;

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
            content.remove(content.len() - 2);
        } else if input_txt.ends_with("$") {
            content.pop();
            end = true;
        }
        Self { start, content, end }
    }
}

impl TextSet {
    pub fn new(input_txt: &str) -> Self {
        Self::from_leaf(TextAstLeaf::new(input_txt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::sets::{Container, Overlappable, SetRelation};

    mod construction {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("cave canem", TextAstLeaf { start: false, content: "cave canem".to_string(), end: false }; "plain substring, no anchors")]
        #[test_case(r"\^nija Glu$m  ", TextAstLeaf { start: false, content: "^nija Glu$m  ".to_string(), end: false }; "escaped leading caret is kept literal")]
        #[test_case(r"yogurt\$", TextAstLeaf { start: false, content: "yogurt$".to_string(), end: false }; "escaped trailing dollar is kept literal")]
        #[test_case(r"\^^\^guspo$\$", TextAstLeaf { start: false, content: r"^^\^guspo$$".to_string(), end: false }; "escaped caret and dollar at both ends")]
        #[test_case("^ ganico ", TextAstLeaf { start: true, content: " ganico ".to_string(), end: false }; "leading caret marks a prefix")]
        #[test_case(r"lemmo$", TextAstLeaf { start: false, content: "lemmo".to_string(), end: true }; "trailing dollar marks a suffix")]
        #[test_case(r"^\^O$$", TextAstLeaf { start: true, content: r"\^O$".to_string(), end: true }; "unescaped anchors at both ends mark an exact match")]
        fn parses_anchors_as_expected(input: &str, expected: TextAstLeaf) {
            assert_eq!(TextAstLeaf::new(input), expected);
        }
    }

    mod containment {
        use super::*;
        use test_case::test_case;

        #[test_case("casa", "Nico si casal de lim"; "plain substring match")]
        #[test_case("^ magnone seli", " magnone seli cumas"; "prefix match")]
        #[test_case("to tha suco$$", "nunca to tha suco$"; "suffix match")]
        #[test_case("^^j$", "^j"; "exact match")]
        fn contains_matching_text(text_set: &str, text: &str) {
            let leaf = TextAstLeaf::new(text_set);
            assert!(leaf.contains(text));
        }

        #[test_case("casa", "Nico si Casas de lim"; "case mismatch breaks a substring match")]
        #[test_case("^ magnone seli", "um magnone seli cumas"; "prefix not at the start fails")]
        #[test_case("to tha suco$$", "nunca to tha suco$ demais"; "suffix not at the end fails")]
        #[test_case("^^j$", ".^j."; "exact match rejects extra characters")]
        fn does_not_contain_non_matching_text(text_set: &str, text: &str) {
            let leaf = TextAstLeaf::new(text_set);
            assert!(!leaf.contains(text));
        }
    }

    mod set_relation {
        use super::*;
        use SetRelation::*;
        use test_case::test_case;

        #[test_case("^lemure$", Equal, "^lemure$"; "equal, both exact")]
        #[test_case("gremure$", Equal, "gremure$"; "equal, both suffix-anchored")]
        #[test_case("^;leMut ", Equal, "^;leMut "; "equal, both prefix-anchored")]
        #[test_case(";Mut ", Equal, ";Mut "; "equal, both plain substrings")]
        #[test_case("^lemure$", Subset, "^lemu"; "subset: exact inside a same-prefix prefix-match")]
        #[test_case("^gremure$", Subset, "mure$"; "subset: exact inside a same-suffix suffix-match")]
        #[test_case("^;leMut fm", Subset, "^;leMut "; "subset: both prefix-anchored, one extends the other")]
        #[test_case(";leMut fm$", Subset, "Mut fm$"; "subset: both suffix-anchored, one extends the other")]
        #[test_case("^;Mut $", Subset, "Mu"; "subset: exact inside a substring")]
        #[test_case("^;Mu", Subset, "Mu"; "subset: prefix-anchored inside a substring")]
        #[test_case("ut $", Subset, "u"; "subset: suffix-anchored inside a substring")]
        #[test_case(" nisp o y-utusv", Subset, "o y-utu"; "subset: both plain substrings, one contains the other")]
        #[test_case("^l emure", Superset, "^l emureti cos(8)$"; "superset: prefix-anchored contains a same-prefix exact match")]
        #[test_case("mure][$", Superset, "^gremure][$"; "superset: suffix-anchored contains a same-suffix exact match")]
        #[test_case("^;leM", Superset, "^;leMut "; "superset: both prefix-anchored, this one is the shorter prefix")]
        #[test_case(" fm$", Superset, "Mut fm$"; "superset: both suffix-anchored, this one is the shorter suffix")]
        #[test_case("t ", Superset, "^;Mut $"; "superset: substring contains an exact match")]
        #[test_case("Mu", Superset, "^;Mut"; "superset: substring contains a prefix-anchored match")]
        #[test_case("kut", Superset, "makut $"; "superset: substring contains a suffix-anchored match")]
        #[test_case("tutu", Superset, "malitutu"; "superset: both plain substrings, this one is contained in the other")]
        #[test_case("^l emure", Overlapping, "cos(8)$"; "overlap: prefix-anchored vs suffix-anchored, unrelated content")]
        #[test_case("mure][$", Overlapping, "^gre"; "overlap: suffix-anchored vs prefix-anchored, unrelated content")]
        #[test_case("^;leM", Overlapping, "giummo"; "overlap: prefix-anchored vs plain substring, unrelated content")]
        #[test_case(" fm$", Overlapping, "giummo"; "overlap: suffix-anchored vs plain substring, unrelated content")]
        #[test_case("dribbo", Overlapping, "^;Mut obbo"; "overlap: plain substring vs prefix-anchored, unrelated content")]
        #[test_case("dribbo", Overlapping, ";Mut fibbo$"; "overlap: plain substring vs suffix-anchored, unrelated content")]
        #[test_case("canimo", Overlapping, ";::::;"; "overlap: both plain substrings, unrelated content")]
        #[test_case("^l emure$", Disjoint, "^cos(8)$"; "disjoint: both exact, different content")]
        #[test_case("^;leM", Disjoint, "^giummo"; "disjoint: both prefix-anchored, different content")]
        #[test_case(" fm$", Disjoint, "giummo$"; "disjoint: both suffix-anchored, different content")]
        #[test_case("^mure][$", Disjoint, "^gre"; "disjoint: exact vs prefix-anchored, incompatible content")]
        #[test_case("^mure][$", Disjoint, "gre$"; "disjoint: exact vs suffix-anchored, incompatible content")]
        #[test_case("^dribbo", Disjoint, "^;Mut obbo$"; "disjoint: prefix-anchored vs exact, incompatible content")]
        #[test_case("dribbo$", Disjoint, "^;Mut obbo$"; "disjoint: suffix-anchored vs exact, incompatible content")]
        fn matches_expected_relation(a: &str, rel: SetRelation, b: &str) {
            assert_eq!(TextAstLeaf::new(a).set_relation(&TextAstLeaf::new(b)), rel);
        }
    }

    mod text_set_construction {
        use super::*;

        #[test]
        fn new_wraps_a_single_leaf_that_matches_like_the_bare_leaf() {
            let set = TextSet::new("^exact$");
            let leaf = TextAstLeaf::new("^exact$");
            assert!(leaf.contains("exact"));
            assert!(set.contains("exact"));
            assert!(!leaf.contains("not exact"));
            assert!(!set.contains("not exact"));
        }
    }
}
