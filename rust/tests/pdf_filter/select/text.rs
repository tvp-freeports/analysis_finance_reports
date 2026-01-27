use freeports_lib::pdf_filter::select::text::*;
use freeports_lib::pdf_filter::select::sets::Container;
use test_case::test_case;
use pretty_assertions::assert_eq;

#[test_case(
    TextSet::new("juk"),
    "text that has to be jukonne";
    "simple"
)]
#[test_case(
    TextSet::new("onne$"),
    "text that has to be jukonne";
    "end"
)]
#[test_case(
    TextSet::new("^text"),
    "text that has to be jukonne";
    "begin"
)]
#[test_case(
    TextSet::new("niluk") | TextSet::new("jukonne"),
    "text that has to be jukonne";
    "union"
)]
#[test_case(
    TextSet::new("to be") & TextSet::new("nne$"),
    "text that has to be jukonne";
    "intersect"
)]
#[test_case(
    TextSet::new("text") / TextSet::new("jukone"),
    "text that has to be jukonne";
    "subtraction"
)]
#[test_case(
    TextSet::new("tra") | (TextSet::new("jukonne") & TextSet::new("to be ") / TextSet::new("pulvilio")),
    "text that has to be jukonne";
    "complex"
)]
fn element_in_textset(txt_set: TextSet, txt: &str){
    assert!(txt_set.contains(txt));
}



#[test_case(
    TextSet::new("junk"),
    "text that has to be jukonne";
    "simple"
)]
#[test_case(
    TextSet::new("has to be$"),
    "text that has to be jukonne";
    "end"
)]
#[test_case(
    TextSet::new("^lex"),
    "text that has to be jukonne";
    "begin"
)]
#[test_case(
    TextSet::new("niluk") | TextSet::new("jukonne"),
    "text that has to be edonne";
    "union"
)]
#[test_case(
    TextSet::new("to be") & TextSet::new("nez$"),
    "text that has to be jukonne";
    "intersect"
)]
#[test_case(
    TextSet::new("text") / TextSet::new("jukonne"),
    "text that has to be jukonne";
    "subtraction"
)]
#[test_case(
    TextSet::new("tra") | (TextSet::new("jukonne") & TextSet::new("to be ") / TextSet::new("be")),
    "text that has to be jukonne";
    "complex"
)]
fn element_not_in_textset(txt_set: TextSet, txt: &str){
    assert!(!txt_set.contains(txt));
}