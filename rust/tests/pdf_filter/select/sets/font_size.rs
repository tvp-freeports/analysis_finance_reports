use freeports_lib::pdf_filter::select::sets::font_size::*;
use freeports_lib::pdf_filter::select::sets::Container;
use test_case::test_case;
use pretty_assertions::assert_eq;

// #[test_case(
//     FontSizeSet::new("juk"),
//     "juk";
//     "simple"
// )]
// #[test_case(
//     FontSet::new("Jucca no.vento"),
//     "      JUCCA/no/vento";
//     "normalized"
// )]
// #[test_case(
//     FontSet::new("THE FROG") | FontSet::new("the.fogg"),
//     "the-fogg";
//     "union"
// )]
// #[test_case(
//     FontSet::new("to be") & FontSet::new("TO BE"),
//     "to/Be";
//     "intersect"
// )]
// #[test_case(
//     FontSet::new("FULMA") / FontSet::new("ghiotto"),
//     " fulma ";
//     "subtraction"
// )]
// #[test_case(
//     (FontSet::new("souvenir") | FontSet::new("Galego")) / (FontSet::new("France") & FontSet::new("malquibo")),
//     "SOUVENIR";
//     "complex"
// )]
// fn element_in_fontset(txt_set: FontSet, txt: &str){
//     assert!(txt_set.contains(txt));
// }


// #[test_case(
//     FontSet::new("hulk"),
//     "juk";
//     "simple"
// )]
// #[test_case(
//     FontSet::new("Jucca no.vento"),
//     "      JUCA/no/vento";
//     "normalized"
// )]
// #[test_case(
//     FontSet::new("THE FROG") | FontSet::new("the.fogg"),
//     "the-smogg";
//     "union"
// )]
// #[test_case(
//     FontSet::new("to be") & FontSet::new("to see"),
//     "to be";
//     "intersect"
// )]
// #[test_case(
//     FontSet::new("FULMA") / FontSet::new("ghiotto"),
//     " casola 8";
//     "subtraction"
// )]
// #[test_case(
//     (FontSet::new("souvenir") | FontSet::new("Galego")) | (FontSet::new("France") & FontSet::new("malquibo")),
//     "malquibo";
//     "complex"
// )]
// fn element_not_in_fontset(txt_set: FontSet, txt: &str){
//     assert!(!txt_set.contains(txt));
// }