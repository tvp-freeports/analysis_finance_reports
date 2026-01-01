use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use pyo3::pyclass;

use std::marker;
use std::collections::{HashMap};
use std::iter::{zip};

use bitflags::bitflags;
use super::{TableConfig};

#[derive(Debug,Clone)]
pub struct Limits(f32, f32);

impl Limits {
    pub fn build(a: f32, b: f32) -> Result<Self,LimitsBuildError> {
        if a < 0.0 {
            Err(LimitsBuildError::NegativeLeftEntry(a))
        } else if b < 0.0 {
            Err(LimitsBuildError::NegativeRightEntry(b))
        } else if a >= b {
            Err(LimitsBuildError::NegativeInterval(a,b,b-a))
        } else {
            Ok(Self(a,b))
        }
    }
}
impl FromPyObject<'_, '_> for Limits {
    type Error = PyErr;
    fn extract(tuple: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
        let a: f32 = tuple.get_item(0)?.extract()?;
        let b: f32 = tuple.get_item(1)?.extract()?;
        Ok(Limits::build(a,b)?)
    }
}


#[derive(Debug)]
pub enum LimitsBuildError {
    NegativeLeftEntry(f32),
    NegativeRightEntry(f32),
    NegativeInterval(f32,f32,f32)
}
impl From<LimitsBuildError> for PyErr {
    fn from(err: LimitsBuildError) -> PyErr {
        PyValueError::new_err(format!("{err:?}"))
    }
}



bitflags!{
    #[pyclass]
    #[derive(Clone,Copy)]
    pub struct TablePosAlgorithm: u8 {
        const Default = 0b000000000;
        const ReturnRows = 0b00000001;
        const BigCellRule = 0b00000010;
        const UseRulerArea = 0b00000100;
        const UseTestPos = 0b00001000;
    }
}
#[pymethods]
impl TablePosAlgorithm {
    /// Create from raw bits
    #[new]
    pub fn new(bits: u8) -> PyResult<Self> {
        Ok(TablePosAlgorithm::from_bits_truncate(bits))
    }
    #[staticmethod]
    pub fn from_flags(flags: Vec<Self>) -> Self {
        flags.into_iter().fold(Self::Default, |a, b| a | b)
    }

    /// Bitwise OR (so Python can do a | b)
    fn __or__(&self, other: &Self) -> Self {
        *self | *other
    }

    fn __repr__(&self) -> String {
        format!("TablePosAlgorithm({:#010b})", self.bits())
    }

    #[classattr]
    const DEFAULT: Self = Self::Default;
    #[classattr]
    const RETURN_ROWS: Self = Self::ReturnRows;
    #[classattr]
    const BIG_CELL_RULE: Self = Self::BigCellRule;
    #[classattr]
    const USE_RULER_AREA: Self = Self::UseRulerArea;
    #[classattr]
    const USE_TEST_POS: Self = Self::UseTestPos;
}
// impl FromPyObject<'_, '_> for TablePosAlgorithm {
//     type Error = PyErr;
//     fn extract(tuple: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
//         let a: f32 = tuple.get_item(0)?.extract()?;
//         let b: f32 = tuple.get_item(1)?.extract()?;
//         Ok(Limits::build(a,b)?)
//     }
// }


#[pyclass]
#[derive(Debug,Clone)]
pub struct CellGeometry {
    bounds: (f32,f32,f32,f32),
    tolerance: f32
}

#[pymethods]
impl CellGeometry {
    #[new]
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

#[derive(Debug,Clone)]
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
) -> Vec<usize> {
    let return_col = !algorithm_flags.contains(TablePosAlgorithm::ReturnRows);
    let mut unindexed: Vec<CellGeometryUnindexed<'a>> = cells.iter().enumerate().map(
        |(i, c)| CellGeometryUnindexed::from_cell_geometry(c, i, return_col)
    ).collect();
    let mut indexes: Vec<Option<usize>> = vec![None; cells.len()];
    let mut rulers: Vec<CellGeometryUnindexed> = Vec::new();
    while indexes.iter().any(|a| a.is_none()) {
        let current_ruler_idx = rulers.len();
        let selected: CellGeometryUnindexed=if !algorithm_flags.contains(TablePosAlgorithm::BigCellRule) {
                unindexed.iter().min_by(
                    |a, b| a.area.partial_cmp(&b.area).unwrap()
                ).unwrap()
            } else {
                unindexed.iter().max_by(
                    |a, b| a.area.partial_cmp(&b.area).unwrap()
                ).unwrap()
            }.clone();
        rulers.push(selected);
        let ruler=&rulers[current_ruler_idx];
        
        unindexed.retain(|elem| {
            let it_matches=if algorithm_flags.contains(TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::UseTestPos) {
                position_in_area(&elem,&ruler)
            } else if algorithm_flags.contains(TablePosAlgorithm::UseRulerArea) {
                areas_intersect(&ruler,&elem)
            } else if algorithm_flags.contains(TablePosAlgorithm::UseTestPos) {
                same_position(&ruler,&elem)
            } else {
                position_in_area(&ruler,&elem)
            };
            if it_matches {
                indexes[elem.index]=Some(current_ruler_idx)
            }
            !it_matches
        });
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
    indexes.iter().map(|x| mapping[&x.unwrap()]).collect()
}

pub fn get_table_coordinates(
    cells: &[CellGeometry],
    algorithm_flags: TablePosAlgorithm,
    table_config: &TableConfig
) -> Vec<(usize,usize)> {
    if algorithm_flags.contains(TablePosAlgorithm::ReturnRows) {
        panic!("Doesn't make any sense to return Row indexes when interested to (Row,Col)")
    }
    let algorithm_flags_rows=algorithm_flags.clone() | TablePosAlgorithm::ReturnRows;
    let cols = get_table_indexes(cells,algorithm_flags,table_config);
    let rows = get_table_indexes(cells,algorithm_flags_rows,table_config);
    zip(rows,cols).collect()
}


#[pyfunction]
#[pyo3(name = "get_table_coordinates")]
pub fn py_get_table_coordinates(
    cells: Vec<CellGeometry>,
    algorithm_flags: TablePosAlgorithm,
    table_config: &TableConfig
) -> Vec<(usize,usize)> {
    get_table_coordinates(&cells,algorithm_flags,table_config)
}


#[cfg(test)]
mod tests {
    use super::*;
    mod limits_build {
        use super::*;
        #[test]
        fn ok() {
            let Limits(a,b) = Limits::build(20.3,30.7).unwrap();
            assert_eq!(a,20.3);
            assert_eq!(b,30.7);
        }
        #[test]
        fn err() {
            assert!(matches!(
                Limits::build(-20.0, 30.1),
                Err(LimitsBuildError::NegativeLeftEntry(-20.0))
            ));
            assert!(matches!(
                Limits::build(20.0, -30.1),
                Err(LimitsBuildError::NegativeRightEntry(-30.1))
            ));
            assert!(matches!(
                Limits::build(30.1, 20.0),
                Err(LimitsBuildError::NegativeInterval(30.1, 20.0, -10.1))
            ));
        }
    }

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
            let cell_a=CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b=CellGeometry::new((0.5,0.0,1.5,1.5),1.1);
            let cell_c=CellGeometry::new((2.0,0.0,10.3,3.2),15.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=vec![
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
            let cell_a=CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b=CellGeometry::new((10.5,0.0,11.5,1.5),1.1);
            let cell_c=CellGeometry::new((2.0,0.0,10.3,3.2),0.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=vec![
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
            let cell_a=CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b=CellGeometry::new((0.5,0.0,1.5,1.5),1.1);
            let cell_c=CellGeometry::new((2.0,0.0,10.3,3.2),15.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=vec![
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
            let cell_a=CellGeometry::new((0.0,0.0,1.0,1.0),0.0);
            let cell_b=CellGeometry::new((10.5,0.0,11.5,1.5),1.1);
            let cell_c=CellGeometry::new((2.0,0.0,10.3,3.2),0.1);
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=vec![
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
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=vec![
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
            let cells_h: Vec<(CellGeometryUnindexed,CellGeometryUnindexed)>=vec![
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
    
    #[test]
    fn cell_geometry_unindexed_from_cell_geometry() {
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
    fn get_index_no_info() {
        let table_cfg=TableConfig{
            cols: None,
            rows: None
        };
        let x_col: Vec<((f32,f32),usize)> = vec![
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
        let y_row: Vec<((f32,f32),usize)> = vec![
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
        let col_indexes: Vec<usize> = x_col.iter().map(|a| a.1).collect();
        let row_indexes: Vec<usize> = y_row.iter().map(|a| a.1).collect();
        let mut cells: Vec<CellGeometry> = Vec::new();
        for i in 0..x_col.len() {
            let (x0,x1)=x_col[i].0;
            let (y0,y1)=y_row[i].0;
            cells.push(
                CellGeometry::new((x0,y0,x1,y1),0.0)
            );
        }
        
        assert_eq!(
            col_indexes,
            get_table_indexes(
                &cells,
                TablePosAlgorithm::Default,
                &table_cfg
            )
        );
        assert_eq!(
            row_indexes,
            get_table_indexes(
                &cells,
                TablePosAlgorithm::ReturnRows,
                &table_cfg
            )
        );
        assert_eq!(
            zip(row_indexes,col_indexes).collect::<Vec<(usize,usize)>>(),
            get_table_coordinates(
                &cells,
                TablePosAlgorithm::Default,
                &table_cfg
            )
        )
    }
    #[test]
    #[should_panic(expected="Doesn't make any sense to return Row indexes when interested to (Row,Col)")]
    fn get_table_coordinates_return_rows(){
        let cells=vec![CellGeometry::new((0.0,1.0,1.0,2.0),0.0)];
        get_table_coordinates(
            &cells,
            TablePosAlgorithm::ReturnRows,
            &TableConfig{
                cols: None,
                rows: None
            }
        );
    }

}