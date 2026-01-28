use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;


use std::marker;
use std::collections::{HashMap};
use std::iter::{zip};

use bitflags::bitflags;
use super::TableConfig;
use crate::commons::geometric::Limits;

// #[derive(Debug,Clone,Copy)]
// pub struct Limits(f32, f32);

// impl Limits {
//     pub fn build(a: f32, b: f32) -> Result<Self,LimitsBuildError> {
//         use LimitsBuildError::*;
//         if a < 0.0 {
//             Err(LeftNegative(a))
//         } else if b < 0.0 {
//             Err(RightNegative(b))
//         } else if a >= b {
//             Err(NegativeInterval(a,b))
//         } else {
//             Ok(Self(a,b))
//         }
//     }
// }
// impl FromPyObject<'_, '_> for Limits {
//     type Error = PyErr;
//     fn extract(tuple: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
//         let a: f32 = tuple.get_item(0)?.extract()?;
//         let b: f32 = tuple.get_item(1)?.extract()?;
//         Ok(Limits::build(a,b)?)
//     }
// }


// #[derive(Debug)]
// pub enum LimitsBuildError {
//     LeftNegative(f32),
//     RightNegative(f32),
//     NegativeInterval(f32,f32)
// }
// impl From<LimitsBuildError> for PyErr {
//     fn from(err: LimitsBuildError) -> PyErr {
//         PyValueError::new_err(format!("{err:?}"))
//     }
// }



#[derive(Debug,Clone,Copy)]
pub enum CoordinateExtractionError {
    MismatchColumnNumber(usize,usize),
    MismatchRowNumber(usize,usize)
}
impl From<CoordinateExtractionError> for PyErr {
    fn from(err: CoordinateExtractionError) -> PyErr {
        use CoordinateExtractionError::*;
        PyValueError::new_err(
            match err {
                MismatchColumnNumber(expected,found) => format!("Expected {expected} columns, found {found}"),
                MismatchRowNumber(expected,found) => format!("Expected {expected} rows, found {found}")
            }
        )
    }
}



bitflags!{
    #[derive(Clone,Copy)]
    pub struct TablePosAlgorithm: u8 {
        const Default = 0b000000000;
        const ReturnRows = 0b00000001;
        const BigCellRule = 0b00000010;
        const UseRulerArea = 0b00000100;
        const UseTestPos = 0b00001000;
    }
}
impl FromPyObject<'_, '_> for TablePosAlgorithm {
    type Error = PyErr;
    fn extract(flags: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
        let mut res=Self::Default;
        for f in flags.try_iter()? {
            let flag_name: String = f?.getattr("name")?.extract()?;
            match flag_name.as_str() {
                "RETURN_ROWS" => res |= Self::ReturnRows,
                "BIG_CELL_RULE" => res |= Self::BigCellRule,
                "USE_RULER_AREA" => res |= Self::UseRulerArea,
                "USE_TEST_POS" => res |= Self::UseTestPos,
                flg => return Err(PyValueError::new_err(format!("TablePosAlgorithm flag {flg} not recognized")))
            }
        }
        Ok(res)
    }
}


#[derive(Debug,Clone,FromPyObject,Copy)]
pub struct CellGeometry {
    bounds: (f32,f32,f32,f32),
    tolerance: f32
}

impl CellGeometry {
    pub fn new(bounds: (f32,f32,f32,f32), tolerance: f32) -> Self {
        let (x0,y0,x1,y1)=bounds;
        if let Err(err)=Limits::build(x0,x1) {
            panic!("Invalid horizontal interval: {err:?}");
        }
        if let Err(err)=Limits::build(y0,y1) {
            panic!("Invalid vertical interval: {err:?}");
        }
        Self{
            bounds,
            tolerance
        }
    }
}

#[derive(Debug,Clone,Copy)]
struct CellGeometryUnindexed<'a>{
    _marker: marker::PhantomData<&'a CellGeometry>,
    index: usize,
    area: f32,
    pos: f32,
    bounds: Limits,
    tolerance: f32,
}
impl<'a> CellGeometryUnindexed<'a> {
    fn from_cell_geometry(cell: &'a CellGeometry, index: usize, horizontal: bool) -> Self {
        let CellGeometry{
            tolerance,
            bounds: (x0,y0,x1,y1) 
        } = cell;
        let (a,b) = if horizontal { (*x0,*x1) } else { (*y0,*y1) };
        Self{
            _marker: marker::PhantomData,
            index,
            tolerance: *tolerance,
            area: b-a,
            pos: (a+b)/2.0,
            bounds: Limits::build(a,b).unwrap(),
        }
    }
    fn from_limits(limits: Limits, index: usize, tolerance: f32) -> Self {
        let Limits(a,b) = limits;
        Self{
            _marker: marker::PhantomData,
            index,
            tolerance,
            area: b-a,
            pos: (a+b)/2.0,
            bounds: limits
        }
    }
}


fn same_position(a: &CellGeometryUnindexed,b: &CellGeometryUnindexed) -> bool {
    (a.pos - b.pos).abs() <= (a.tolerance+b.tolerance)/2.0
}
fn position_in_area(a: &CellGeometryUnindexed,b: &CellGeometryUnindexed) -> bool {
    let Limits(l,r)=b.bounds;
    let t=(a.tolerance+b.tolerance)/2.0;
    a.pos <= r+t && a.pos >= l-t
}
fn areas_intersect(a: &CellGeometryUnindexed,b: &CellGeometryUnindexed) -> bool {
    let Limits(_al,_ar)=a.bounds;
    let Limits(_bl,_br)=b.bounds;
    let (al,ar,bl,br)=(
        _al-a.tolerance,_ar+a.tolerance,_bl-b.tolerance,_br+b.tolerance
    );
    
    ar >= bl && al <= br
}

fn get_table_indexes<'a>(
    cells: &'a[CellGeometry],
    algorithm_flags: TablePosAlgorithm,
    table_config: &TableConfig
) -> Result<Vec<usize>,CoordinateExtractionError> {
    let return_col = !algorithm_flags.contains(TablePosAlgorithm::ReturnRows);
    let mut unindexed: Vec<CellGeometryUnindexed<'a>> = cells.iter().enumerate().map(
        |(i, c)| CellGeometryUnindexed::from_cell_geometry(c, i, return_col)
    ).collect();
    let mut indexes: Vec<Option<usize>> = vec![None; cells.len()];
    let mut rulers: Vec<CellGeometryUnindexed> = Vec::new();
    let cfg: Option<Vec<Option<Limits>>> = if return_col {
        table_config.cols.as_ref().map(|conf| conf.iter().map(|c| c.limits).collect())
    } else {
        table_config.rows.as_ref().map(|conf| conf.iter().map(|c| c.limits).collect())
    };
    let n_expected_indexes: Option<usize> = cfg.as_ref().map(|l| l.len());
    let mut cfg_iter=cfg.map(|v| v.into_iter());
    while indexes.iter().any(|a| a.is_none()) {
        let limits: Option<Limits> = cfg_iter.as_mut().and_then(|i| i.next()).flatten();
        let current_ruler_idx = rulers.len();

        let selected: CellGeometryUnindexed= match limits {
            Some(l) => {
                CellGeometryUnindexed::from_limits(l,0,0.0)
            },
            None => * if !algorithm_flags.contains(TablePosAlgorithm::BigCellRule) {
                unindexed.iter().min_by(
                    |a, b| a.area.partial_cmp(&b.area).unwrap()
                ).unwrap()
            } else {
                unindexed.iter().max_by(
                    |a, b| a.area.partial_cmp(&b.area).unwrap()
                ).unwrap()
            }
        };
        rulers.push(selected);
        let ruler=&rulers[current_ruler_idx];
        unindexed.retain(|elem| {
            let it_matches=if algorithm_flags.contains(TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::UseTestPos) {
                position_in_area(elem,ruler)
            } else if algorithm_flags.contains(TablePosAlgorithm::UseRulerArea) {
                areas_intersect(ruler,elem)
            } else if algorithm_flags.contains(TablePosAlgorithm::UseTestPos) {
                same_position(ruler,elem)
            } else {
                position_in_area(ruler,elem)
            };
            if it_matches {
                indexes[elem.index]=Some(current_ruler_idx)
            }
            !it_matches
        });
    }
    if let Some(n_expected_indexes) = n_expected_indexes {
        let n_indexes = rulers.len();
        if n_indexes != n_expected_indexes {
            if return_col {
                return Err(
                    CoordinateExtractionError::MismatchColumnNumber(n_expected_indexes,n_indexes)
                );
            } else {
                return Err(
                    CoordinateExtractionError::MismatchRowNumber(n_expected_indexes,n_indexes)
                );
            }
        }
    }
    let mut unordered_mapping: Vec<(usize,CellGeometryUnindexed)> = rulers
    .into_iter()
    .enumerate()
    .collect();
    unordered_mapping.sort_by(|a,b| b.1.pos.partial_cmp(&a.1.pos).unwrap() );
    unordered_mapping.reverse();
    let mut mapping: HashMap<usize,usize> = HashMap::new();
    for (i,pos) in unordered_mapping
            .into_iter()
            .map(|(i,_r)| i)
            .enumerate() {
        mapping.insert(pos,i);
    }
    Ok(indexes.iter().map(|x| mapping[&x.unwrap()]).collect())
}

pub fn get_table_coordinates(
    cells: &[CellGeometry],
    algorithm_flags: TablePosAlgorithm,
    table_config: &TableConfig
) -> Result<Vec<(usize,usize)>,CoordinateExtractionError> {
    if algorithm_flags.contains(TablePosAlgorithm::ReturnRows) {
        panic!("Doesn't make any sense to return Row indexes when interested to (Row,Col)")
    }
    let algorithm_flags_rows=algorithm_flags | TablePosAlgorithm::ReturnRows;
    let cols = get_table_indexes(cells,algorithm_flags,table_config)?;
    let rows = get_table_indexes(cells,algorithm_flags_rows,table_config)?;
    Ok(zip(rows,cols).collect())
}


#[pyfunction]
#[pyo3(name = "get_table_coordinates")]
pub fn py_get_table_coordinates(
    cells: Vec<CellGeometry>,
    algorithm_flags: TablePosAlgorithm,
    table_config: TableConfig
) -> Result<Vec<(usize,usize)>,CoordinateExtractionError> {
    get_table_coordinates(&cells,algorithm_flags,&table_config)
}


#[cfg(test)]
mod tests {
    
    use super::*;
    use super::super::{ColumnConfig,RowConfig};
    
    // mod limits_build {
    //     use super::*;
    //     #[test]
    //     fn ok() {
    //         assert!(matches!(
    //             Limits::build(20.3,30.7),
    //             Ok(Limits(20.3,30.7))
    //         ));
    //     }
    //     #[test]
    //     fn err() {
    //         use LimitsBuildError::*;
    //         assert!(matches!(
    //             Limits::build(-20.0, 30.1),
    //             Err(LeftNegative(-20.0))
    //         ));
    //         assert!(matches!(
    //             Limits::build(20.0, -30.1),
    //             Err(RightNegative(-30.1))
    //         ));
    //         assert!(matches!(
    //             Limits::build(30.1, 20.0),
    //             Err(NegativeInterval(30.1, 20.0))
    //         ));
    //     }
    // }

    mod cell_geometry_new {
        use super::*;
        #[test]
        fn success() {
            assert!(matches!(
                CellGeometry::new((0.1,2.0,40.0,22.0),43.0),
                CellGeometry{
                    bounds: (0.1,2.0,40.0,22.0),
                    tolerance: 43.0
                }
            ))
        }
        #[test]
        #[should_panic(expected="Invalid horizontal interval:")]
        fn panic_horizontal_interval() {
            CellGeometry::new((-0.1,2.0,40.0,22.0),43.0);
        }
        #[test]
        #[should_panic(expected="Invalid vertical interval:")]
        fn panic_vertical_interval() {
            CellGeometry::new((0.1,22.0,40.0,2.0),43.0);
        }
    }
    mod table_coordinate_alghoritms {
        use super::*;
        #[test]
        fn same_position_true() {
            let cell_a = CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b = CellGeometry::new((0.5,0.0,1.5,1.5),1.1);
            let cell_c = CellGeometry::new((2.0,0.0,10.3,3.2),15.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>  = [
                (&cell_a,&cell_b),
                (&cell_b,&cell_a),
                (&cell_a,&cell_c),
                (&cell_b,&cell_c),
                (&cell_c,&cell_a),
                (&cell_c,&cell_b),
            ].iter().map(|(a,b)| (
                CellGeometryUnindexed::from_cell_geometry(a,10,true),
                CellGeometryUnindexed::from_cell_geometry(b,12,true)
            )).collect();
            for (a,b) in cells_h {
                assert!(same_position(&a,&b))
            }

        }
        #[test]
        fn same_position_false() {
            let cell_a = CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b = CellGeometry::new((10.5,0.0,11.5,1.5),1.1);
            let cell_c = CellGeometry::new((2.0,0.0,10.3,3.2),0.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)> = [
                (&cell_a,&cell_b),
                (&cell_b,&cell_a),
                (&cell_a,&cell_c),
                (&cell_b,&cell_c),
                (&cell_c,&cell_a),
                (&cell_c,&cell_b),
            ].iter().map(|(a,b)| (
                CellGeometryUnindexed::from_cell_geometry(a,10,true),
                CellGeometryUnindexed::from_cell_geometry(b,12,true)
            )).collect();
            for (a,b) in cells_h {
                assert!(!same_position(&a,&b))
            }

        }
        #[test]
        fn position_in_area_true() {
            let cell_a: CellGeometry = CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b: CellGeometry = CellGeometry::new((0.5,0.0,1.5,1.5),1.1);
            let cell_c: CellGeometry = CellGeometry::new((2.0,0.0,10.3,3.2),15.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=[
                (&cell_a,&cell_b),
                (&cell_b,&cell_a),
                (&cell_a,&cell_c),
                (&cell_b,&cell_c),
                (&cell_c,&cell_a),
                (&cell_c,&cell_b),
            ].iter().map(|(a,b)| (
                CellGeometryUnindexed::from_cell_geometry(a,10,true),
                CellGeometryUnindexed::from_cell_geometry(b,12,true)
            )).collect();
            for (a,b) in cells_h {
                assert!(position_in_area(&a,&b))
            }

        }
        #[test]
        fn position_in_area_false() {
            let cell_a: CellGeometry = CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b: CellGeometry = CellGeometry::new((10.5,0.0,11.5,1.5),1.1);
            let cell_c: CellGeometry = CellGeometry::new((2.0,0.0,10.3,3.2),0.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=[
                (&cell_a,&cell_b),
                (&cell_b,&cell_a),
                (&cell_a,&cell_c),
                (&cell_b,&cell_c),
                (&cell_c,&cell_a),
                (&cell_c,&cell_b),
            ].iter().map(|(a,b)| (
                CellGeometryUnindexed::from_cell_geometry(a,10,true),
                CellGeometryUnindexed::from_cell_geometry(b,12,true)
            )).collect();
            for (a,b) in cells_h {
                assert!(!position_in_area(&a,&b))
            }

        }
        #[test]
        fn areas_intersect_true() {
            let cell_a=CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b=CellGeometry::new((1.1,0.0,10.5,1.5),0.2);
            let cell_c=CellGeometry::new((2.0,0.0,10.3,3.2),15.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=[
                (&cell_a,&cell_b),
                (&cell_b,&cell_a),
                (&cell_a,&cell_c),
                (&cell_b,&cell_c),
                (&cell_c,&cell_a),
                (&cell_c,&cell_b),
            ].iter().map(|(a,b)| (
                CellGeometryUnindexed::from_cell_geometry(a,10,true),
                CellGeometryUnindexed::from_cell_geometry(b,12,true)
            )).collect();
            for (a,b) in cells_h {
                assert!(areas_intersect(&a,&b))
            }

        }
        #[test]
        fn areas_intersect_false() {
            let cell_a=CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b=CellGeometry::new((10.5,0.0,11.5,1.5),1.1);
            let cell_c=CellGeometry::new((13.0,0.0,13.3,3.2),0.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=[
                (&cell_a,&cell_b),
                (&cell_b,&cell_a),
                (&cell_a,&cell_c),
                (&cell_b,&cell_c),
                (&cell_c,&cell_a),
                (&cell_c,&cell_b),
            ].iter().map(|(a,b)| (
                CellGeometryUnindexed::from_cell_geometry(a,10,true),
                CellGeometryUnindexed::from_cell_geometry(b,12,true)
            )).collect();
            for (a,b) in cells_h {
                assert!(!areas_intersect(&a,&b))
            }

        }
    }
    
    mod cell_geometry_unindexed {
        use super::*;
        #[test]
        fn from_cell_geometry() {
            let cell_geometry=CellGeometry::new((0.1,2.0,40.0,22.0),43.0);
            assert!(matches!(
                CellGeometryUnindexed::from_cell_geometry(&cell_geometry,100,true),
                CellGeometryUnindexed{
                    index: 100,
                    pos: 20.05,
                    area: 39.9,
                    tolerance: 43.0,
                    bounds: Limits(0.1,40.0),
                    _marker: marker::PhantomData
                }
            ));
            assert!(matches!(
                CellGeometryUnindexed::from_cell_geometry(&cell_geometry,11,false),
                CellGeometryUnindexed{
                    index: 11,
                    pos: 12.0,
                    area: 20.0,
                    tolerance: 43.0,
                    bounds: Limits(2.0,22.0),
                    _marker: marker::PhantomData
                }
            ));
        }

        #[test]
        fn from_limits() {
            let cell_limits=Limits::build(0.1,40.5).unwrap();
            assert!(matches!(
                CellGeometryUnindexed::from_limits(cell_limits,99,5.7),
                CellGeometryUnindexed{
                    index: 99,
                    pos: 20.3,
                    area: 40.4,
                    tolerance: 5.7,
                    bounds: Limits(0.1,40.5),
                    _marker: marker::PhantomData
                }
            ));
        }
    }
    
    mod get_index {
        use pretty_assertions::{assert_eq};
        use super::*;
        const X_COL: [((f32,f32),usize); 9] = [
            ((0.0,2.0),0),
            ((2.0,3.0),1),
            ((3.0,4.0),2),
            ((3.0,4.0),2),
            ((0.0,2.0),0),
            ((2.0,3.0),1),
            ((0.0,2.0),0),
            ((3.0,4.0),2),
            ((2.0,3.0),1),
        ];
        const Y_ROW: [((f32,f32),usize); 9] = [
            ((10.0,20.0),0),
            ((20.0,30.0),1),
            ((10.0,20.0),0),
            ((30.0,40.0),2),
            ((20.0,30.0),1),
            ((10.0,20.0),0),
            ((20.0,30.0),1),
            ((30.0,40.0),2),
            ((30.0,40.0),2),
        ];
        #[test]
        fn no_info() {
            let col_indexes: [usize; 9] = X_COL.each_ref().map(|a| a.1);
            let row_indexes: [usize; 9] = Y_ROW.each_ref().map(|a| a.1);
            let cells: Vec<CellGeometry> = {
                X_COL.each_ref().iter().zip(Y_ROW.each_ref())
                    .map(|(&x, &y)| {
                        let (x0, x1) = x.0;
                        let (y0, y1) = y.0;
                        CellGeometry::new((x0, y0, x1, y1), 0.0)
                    })
                    .collect()
            };
            let table_cfg=TableConfig{
                cols: None,
                rows: None
            };
            assert_eq!(
                col_indexes.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::Default,
                    &table_cfg
                ).unwrap()
            );
            assert_eq!(
                row_indexes.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::ReturnRows,
                    &table_cfg
                ).unwrap()
            );
            assert_eq!(
                zip(row_indexes,col_indexes).collect::<Vec<(usize,usize)>>(),
                get_table_coordinates(
                    &cells,
                    TablePosAlgorithm::Default,
                    &table_cfg
                ).unwrap()
            )
        }
        #[test]
        fn no_info_on_cols_and_rows() {
            let col_indexes: [usize; 9] = X_COL.each_ref().map(|a| a.1);
            let row_indexes: [usize; 9] = Y_ROW.each_ref().map(|a| a.1);
            let cells: Vec<CellGeometry> = {
                X_COL.each_ref().iter().zip(Y_ROW.each_ref())
                    .map(|(&x, &y)| {
                        let (x0, x1) = x.0;
                        let (y0, y1) = y.0;
                        CellGeometry::new((x0, y0, x1, y1), 0.0)
                    })
                    .collect()
            };
            let table_cfg=TableConfig{
                cols: Some(vec![
                    ColumnConfig{
                        limits: None,
                        splitting: None,
                        nullable: None
                    }; 3
                ]),
                rows: None
            };
            assert_eq!(
                col_indexes.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::Default,
                    &table_cfg
                ).unwrap()
            );
            let table_cfg=TableConfig{
                cols: Some(vec![
                    ColumnConfig{
                        limits: None,
                        splitting: None,
                        nullable: None
                    }; 3
                ]),
                rows: Some(vec![
                    RowConfig{
                        limits: None
                    }; 3
                ]),
            };
            assert_eq!(
                row_indexes.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::ReturnRows,
                    &table_cfg
                ).unwrap()
            );
        }
        #[test]
        fn get_index_mismatch_cols() {
            let cells: Vec<CellGeometry> = {
                X_COL.each_ref().iter().zip(Y_ROW.each_ref())
                    .map(|(&x, &y)| {
                        let (x0, x1) = x.0;
                        let (y0, y1) = y.0;
                        CellGeometry::new((x0, y0, x1, y1), 0.0)
                    })
                    .collect()
            };
            let table_cfg=TableConfig{
                cols: Some(vec![
                    ColumnConfig{
                        limits: None,
                        splitting: None,
                        nullable: None
                    }; 4
                ]),
                rows: None
            };
            assert!(
                matches!(
                    get_table_indexes(
                        &cells,
                        TablePosAlgorithm::Default,
                        &table_cfg
                    ),
                    Err(CoordinateExtractionError::MismatchColumnNumber(4,3))
                )
            );
        }
        #[test]
        fn get_index_mismatch_rows() {
            let cells: Vec<CellGeometry> = {
                X_COL.each_ref().iter().zip(Y_ROW.each_ref())
                    .map(|(&x, &y)| {
                        let (x0, x1) = x.0;
                        let (y0, y1) = y.0;
                        CellGeometry::new((x0, y0, x1, y1), 0.0)
                    })
                    .collect()
            };
            let table_cfg=TableConfig{
                cols: None,
                rows: Some(vec![
                    RowConfig{
                        limits: None
                    }; 2
                ])
            };
            assert!(
                matches!(
                    get_table_indexes(
                        &cells,
                        TablePosAlgorithm::ReturnRows,
                        &table_cfg
                    ),
                    Err(CoordinateExtractionError::MismatchRowNumber(2,3))
                )
            );
        }
        const INTERVALS: [(f32,f32); 9] = [
            (0.0,2.0),
            (0.5,1.5),
            (0.0,1.8),
            (0.7,1.0),
            // center
            (1.9,3.0),
            (2.0,3.0),
            // right
            (10.0,20.0),
            (30.0,40.0),
            (35.0,45.0),
        ];
        const BIG_ONE: [usize; 9] = [0; 9];
        const ALL_TREE_SX: [usize; 9] = [0,0,0,0,0,0,1,1,2];
        const ALL_TREE_DX: [usize; 9] = [0,0,0,0,1,1,1,2,2];
        const CENTER_UNSPECIFIED: [usize; 9] = [0,0,0,0,1,1,2,2,2];
        #[test]
        fn info_on_limits(){
            let cells=INTERVALS.each_ref().map(
                |&(x0, x1)| CellGeometry::new((x0, 0.0, x1, 1.0), 0.0)
            ).to_vec();
            let table_cfg=TableConfig{
                cols: Some(vec![ColumnConfig{
                    splitting: None,
                    nullable: None,
                    limits: Some(Limits::build(0.1,50.0).unwrap())
                }]),
                rows: None
            };
            assert_eq!(
                BIG_ONE.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::UseRulerArea,
                    &table_cfg
                ).unwrap()
            );
            let cells=INTERVALS.each_ref().map(
                |&(y0, y1)| CellGeometry::new((0.0, y0, 1.0, y1), 0.0)
            ).to_vec();
            let table_cfg=TableConfig{
                rows: Some(vec![
                    RowConfig{limits: Some(Limits::build(0.0,3.0).unwrap())},
                    RowConfig{limits: Some(Limits::build(10.0,35.0).unwrap())},
                    RowConfig{limits: Some(Limits::build(35.0,50.0).unwrap())}
                ]),
                cols: None
            };
            assert_eq!(
                ALL_TREE_SX.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::ReturnRows | TablePosAlgorithm::UseTestPos,
                    &table_cfg
                ).unwrap()
            );
            let table_cfg=TableConfig{
                rows: Some(vec![
                    RowConfig{limits: Some(Limits::build(0.0,2.0).unwrap())},
                    RowConfig{limits: Some(Limits::build(2.0,20.0).unwrap())},
                    RowConfig{limits: Some(Limits::build(20.0,50.0).unwrap())}
                ]),
                cols: None
            };
            assert_eq!(
                ALL_TREE_DX.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::ReturnRows | TablePosAlgorithm::UseTestPos,
                    &table_cfg
                ).unwrap()
            );
            let table_cfg=TableConfig{
                rows: Some(vec![
                    RowConfig{limits: Some(Limits::build(0.0,2.0).unwrap())},
                    RowConfig{limits: None},
                    RowConfig{limits: Some(Limits::build(3.0,50.0).unwrap())}
                ]),
                cols: None
            };
            assert_eq!(
                CENTER_UNSPECIFIED.to_vec(),
                get_table_indexes(
                    &cells,
                    TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::ReturnRows | TablePosAlgorithm::UseTestPos,
                    &table_cfg
                ).unwrap()
            );

        }

    }
    

    #[test]
    #[should_panic(expected="Doesn't make any sense to return Row indexes when interested to (Row,Col)")]
    fn get_table_coordinates_return_rows(){
        let cells=vec![CellGeometry::new((0.0,1.0,1.0,2.0),0.0)];
        let _ = get_table_coordinates(
            &cells,
            TablePosAlgorithm::ReturnRows,
            &TableConfig{
                cols: None,
                rows: None
            }
        );
    }

}