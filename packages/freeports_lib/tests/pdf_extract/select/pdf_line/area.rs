use freeports_lib::pdf_extract::select::pdf_line::area::*;
use freeports_lib::commons::sets::Container;
use test_case::test_case;
use pretty_assertions::assert_eq;

#[test_case(
    AreaSet::new(100.0,200.0,1000.0,2000.0),
    (150.0,398.231);
    "simple"
)]
#[test_case(
    AreaSet::new(1.0,2.0,3.0,4.0) | AreaSet::new(10.0,20.0,30.0,40.0),
    (11.0,21.0);
    "union"
)]
#[test_case(
    AreaSet::new(2.0,0.0,20.0,0.1) & AreaSet::new(15.0,0.0,25.0,0.4),
    (17.4,0.06);
    "intersect"
)]
#[test_case(
    AreaSet::new(40.0,50.0,60.0,150.0) / AreaSet::new(45.0,100.0,55.0,200.0),
    (42.0,75.0);
    "subtraction"
)]
#[test_case(
    AreaSet::new(0.0,90.9,1000.0,900.9) / (AreaSet::new(20.0,0.0,80.0,4000.2) / AreaSet::new(50.0,50.0,60.0,600.0) | AreaSet::new(100.0,30.0,1000.0,80.0) ),
    (55.5,99.0);
    "complex"
)]
fn element_in_fontsizeset(interval: AreaSet, point: (f32,f32)){
    assert!(interval.contains(&point));
}

#[test_case(
    AreaSet::new(100.0,200.0,1000.0,2000.0),
    (1500.0,398.231);
    "simple"
)]
#[test_case(
    AreaSet::new(1.0,2.0,3.0,4.0) | AreaSet::new(10.0,20.0,30.0,40.0),
    (110.0,210.0);
    "union"
)]
#[test_case(
    AreaSet::new(2.0,0.0,20.0,0.1) & AreaSet::new(15.0,0.0,25.0,0.4),
    (17.4,0.3);
    "intersect"
)]
#[test_case(
    AreaSet::new(40.0,50.0,60.0,150.0) / AreaSet::new(45.0,100.0,55.0,200.0),
    (46.0,140.0);
    "subtraction"
)]
#[test_case(
    AreaSet::new(0.0,90.9,1000.0,900.9) / (AreaSet::new(20.0,0.0,80.0,4000.2) / AreaSet::new(50.0,50.0,60.0,600.0) | AreaSet::new(100.0,30.0,1000.0,80.0) ),
    (55.5,990.0);
    "complex"
)]
fn element_not_in_fontsizeset(interval: AreaSet, point: (f32,f32)){
    assert!(!interval.contains(&point));
}




// #[test_case(
//     AreaSet::new(100.0,200.0,1000.0,2000.0),
//     Rectangle::new(150.0,398.231,765.0,1982.0);
//     "simple"
// )]
// #[test_case(
//     AreaSet::new(1.0,2.0,3.0,4.0) | AreaSet::new(10.0,20.0,30.0,40.0),
//     Rectangle::new(11.0,21.0,12.0,22.0);
//     "union"
// )]
// #[test_case(
//     AreaSet::new(2.0,20.0) & AreaSet::new(15.0,25.0),
//     17.4;
//     "intersect"
// )]
// #[test_case(
//     AreaSet::new(40.0,60.0) / AreaSet::new(45.0,55.0),
//     42.0;
//     "subtraction"
// )]
// #[test_case(
//     AreaSet::new(0.0,1000.0) / (AreaSet::new(20.0,80.0) / AreaSet::new(50.0,60.0) | AreaSet::new(100.0,1000.0) ),
//     55.5;
//     "complex"
// )]
// fn element_in_fontsizeset(interval: AreaSet, x: f32){
//     assert!(interval.contains(&x));
// }


// #[test_case(
//     FontSizeSet::new(100.0,200.0),
//     50.0;
//     "simple"
// )]
// #[test_case(
//     FontSizeSet::new(1.0,2.0) | FontSizeSet::new(3.0,4.0),
//     34.5;
//     "union"
// )]
// #[test_case(
//     FontSizeSet::new(2.0,20.0) & FontSizeSet::new(15.0,25.0),
//     3.4;
//     "intersect"
// )]
// #[test_case(
//     FontSizeSet::new(40.0,60.0) / FontSizeSet::new(45.0,55.0),
//     46.0;
//     "subtraction"
// )]
// #[test_case(
//     FontSizeSet::new(0.0,1000.0) / (FontSizeSet::new(20.0,80.0) / FontSizeSet::new(50.0,60.0) | FontSizeSet::new(100.0,1000.0) ),
//     100.1;
//     "complex"
// )]
// fn element_not_in_fontsizeset(interval: FontSizeSet, x: f32){
//     assert!(!interval.contains(&x));
// }

