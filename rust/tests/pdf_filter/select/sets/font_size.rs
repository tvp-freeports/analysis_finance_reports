use freeports_lib::pdf_filter::select::sets::font_size::*;
use freeports_lib::pdf_filter::select::sets::Container;
use test_case::test_case;
use pretty_assertions::assert_eq;

#[test_case(
    FontSizeSet::new(100.0,200.0),
    150.0;
    "simple"
)]
#[test_case(
    FontSizeSet::new(1.0,2.0) | FontSizeSet::new(3.0,4.0),
    3.5;
    "union"
)]
#[test_case(
    FontSizeSet::new(2.0,20.0) & FontSizeSet::new(15.0,25.0),
    17.4;
    "intersect"
)]
#[test_case(
    FontSizeSet::new(40.0,60.0) / FontSizeSet::new(45.0,55.0),
    42.0;
    "subtraction"
)]
#[test_case(
    FontSizeSet::new(0.0,1000.0) / (FontSizeSet::new(20.0,80.0) / FontSizeSet::new(50.0,60.0) | FontSizeSet::new(100.0,1000.0) ),
    55.5;
    "complex"
)]
fn element_in_fontsizeset(interval: FontSizeSet, x: f32){
    assert!(interval.contains(&x));
}


#[test_case(
    FontSizeSet::new(100.0,200.0),
    50.0;
    "simple"
)]
#[test_case(
    FontSizeSet::new(1.0,2.0) | FontSizeSet::new(3.0,4.0),
    34.5;
    "union"
)]
#[test_case(
    FontSizeSet::new(2.0,20.0) & FontSizeSet::new(15.0,25.0),
    3.4;
    "intersect"
)]
#[test_case(
    FontSizeSet::new(40.0,60.0) / FontSizeSet::new(45.0,55.0),
    46.0;
    "subtraction"
)]
#[test_case(
    FontSizeSet::new(0.0,1000.0) / (FontSizeSet::new(20.0,80.0) / FontSizeSet::new(50.0,60.0) | FontSizeSet::new(100.0,1000.0) ),
    100.1;
    "complex"
)]
fn element_not_in_fontsizeset(interval: FontSizeSet, x: f32){
    assert!(!interval.contains(&x));
}

