//! InputArea, RowConfig, ColumnConfig, TableConfig, get_groups.
//!
//! Il vecchio riferimento (`freeports_core`) tiene `RowConfig`/`ColumnConfig`/`TableConfig` in
//! `tabularizer.rs` come semplici `#[derive(FromPyObject)]` senza logica propria, e tiene
//! `InputArea`/`get_groups` in un modulo `position.rs` che nel frattempo era gia' quasi tutto
//! confine PyO3 (validazione via Pydantic-equivalente lato Rust, ma pensata per essere costruita
//! da Python). Questa milestone riassegna deliberatamente `RowConfig`/`ColumnConfig`/
//! `TableConfig` a questo modulo (`position`, non `tabularizer`) e ne rimuove ogni traccia di
//! PyO3: sono dati di configurazione puri, letti in futuro da `formats_repo` (M7) e usati da
//! `tabularizer::{collapse,coordinates}` (M3, stesso livello).
//!
//! **Decisione R2 (`PLAN.md`)**: `CellGeometry` **non** e' ridefinito qui. Il tipo canonico e
//! validato vive in `tabularizer::coordinates` (quello usato realmente dall'algoritmo); questo
//! modulo non lo tocca perche' `RowConfig`/`ColumnConfig`/`TableConfig` non contengono
//! `CellGeometry` — descrivono solo limiti/comportamento di collasso per riga/colonna, non le
//! celle stesse.
//!
//! Un solo `thiserror::Error` per il modulo (`PLAN.md` §8): `PositionError` copre sia la
//! validazione di `InputArea` sia il caso limite di `get_groups` con lista vuota.
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub struct RowConfig { pub limits: Option<Limits> }` — nessuna validazione propria oltre
//!   quella gia' fatta da `Limits::build` a monte; deriva almeno `Debug, Clone, Copy, PartialEq`.
//! - `pub struct ColumnConfig { pub limits: Option<Limits>, pub splitting:
//!   Option<tabularizer::collapse::SplittingState>, pub nullable:
//!   Option<tabularizer::collapse::NullableState> }` — stessi campi del riferimento (spostato
//!   di modulo), stesse derive di `RowConfig`.
//! - `pub struct TableConfig { pub cols: Option<Vec<ColumnConfig>>, pub rows:
//!   Option<Vec<RowConfig>> }` — deriva almeno `Debug, Clone`.
//! - `pub struct InputArea { x_min: Option<f32>, x_max: Option<f32>, y_min: Option<f32>, y_max:
//!   Option<f32> }` (campi privati fuori dal modulo, leggibili dai test annidati). A differenza
//!   del riferimento (Pydantic `PositiveFloat` via `#[new]` PyO3 che solleva `PyErr`), qui **non
//!   c'e' costruttore panicante**: `InputArea` arriva da configurazione esterna (YAML di formato,
//!   M7), quindi la validazione e' sempre un `Result`, mai un panic (coerente con `PLAN.md` §2
//!   principio 4 — a differenza di `Limits`/`Rectangle`, che sono invarianti geometriche interne
//!   al crate e percio' restano panicanti per decisione pregressa).
//!   - Usa `f32`, non `f64` come il vecchio riferimento Pydantic-facing: coerente con il resto di
//!     `pdf_extract` (`PdfLine`/`Rectangle`/`Limits`/`CellGeometry` sono tutti `f32`).
//!   - `InputArea::build(x_min: Option<f32>, x_max: Option<f32>, y_min: Option<f32>, y_max:
//!     Option<f32>) -> Result<Self, PositionError>`: ciascun limite presente deve essere
//!     strettamente positivo (`> 0.0`, come `PositiveFloat`: **non** `>= 0.0`); se sia il minimo
//!     sia il massimo di un asse sono presenti, il massimo deve essere strettamente maggiore del
//!     minimo. Ordine di validazione (per determinismo dei test sugli errori):
//!     `x_min, x_max, y_min, y_max` prima dei controlli incrociati `x_max > x_min`,
//!     `y_max > y_min`.
//!   - Accessori: `x_min(&self) -> Option<f32>`, `x_max`, `y_min`, `y_max` (stessa forma).
//! - `pub enum PositionError { XMinNotPositive(f32), XMaxNotPositive(f32), YMinNotPositive(f32),
//!   YMaxNotPositive(f32), XBoundsInverted{x_min: f32, x_max: f32}, YBoundsInverted{y_min: f32,
//!   y_max: f32}, EmptyLines }`, `thiserror::Error`, derives at least `Debug` (i test qui sotto
//!   fanno `{err:?}` per i messaggi di panico e pattern-match sui campi, non su un valore
//!   letterale in virgola mobile — vietato dal compilatore).
//! - `pub fn get_groups(lines: &[PdfLine], threshold: f32, vertical: bool) -> Result<Vec<i64>,
//!   PositionError>`: prende la coordinata `bbox.1` (`y0`, se `vertical`) o `bbox.0` (`x0`, se
//!   non `vertical`) di ciascuna riga, le ordina, e assegna un id di gruppo crescente ogni volta
//!   che due valori consecutivi (nell'ordine *ordinato*) distano almeno `threshold` fra loro.
//!   **Il vettore risultato e' nell'ordine delle chiavi ordinate, non nell'ordine di `lines` in
//!   ingresso** — comportamento del riferimento preservato deliberatamente (non e' un
//!   miglioramento da applicare, vedi `PLAN.md` §0). `Err(PositionError::EmptyLines)` se `lines`
//!   e' vuoto (il riferimento sollevava `IndexError` allo stesso punto; qui diventa un errore
//!   tipizzato invece di un panic, coerente con `PLAN.md` §2 principio 4).

use crate::commons::geometry::Limits;

use super::pdf_line::PdfLine;

// `SplittingDirection`/`SplittingState`/`NullableState` sono definiti qui (non in
// `tabularizer::collapse`) per evitare una dipendenza circolare fra moduli: `ColumnConfig` ha
// bisogno di questi tipi per i propri campi, e `tabularizer::collapse` ha bisogno di
// `ColumnConfig`/`TableConfig`. Una sola direzione (`tabularizer` -> `position`) e' sufficiente;
// `tabularizer::collapse` li re-esporta dal proprio percorso originale per compatibilita'.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplittingDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplittingState {
    Allow(SplittingDirection),
    Disallow,
}

pub type NullableState = bool;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowConfig {
    pub limits: Option<Limits>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnConfig {
    pub limits: Option<Limits>,
    pub splitting: Option<SplittingState>,
    pub nullable: Option<NullableState>,
}

#[derive(Debug, Clone)]
pub struct TableConfig {
    pub cols: Option<Vec<ColumnConfig>>,
    pub rows: Option<Vec<RowConfig>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PositionError {
    #[error("x_min must be positive, found {0}")]
    XMinNotPositive(f32),
    #[error("x_max must be positive, found {0}")]
    XMaxNotPositive(f32),
    #[error("y_min must be positive, found {0}")]
    YMinNotPositive(f32),
    #[error("y_max must be positive, found {0}")]
    YMaxNotPositive(f32),
    #[error("x_max ({x_max}) must be greater than x_min ({x_min})")]
    XBoundsInverted { x_min: f32, x_max: f32 },
    #[error("y_max ({y_max}) must be greater than y_min ({y_min})")]
    YBoundsInverted { y_min: f32, y_max: f32 },
    #[error("get_groups called with an empty list of lines")]
    EmptyLines,
}

/// Area rettangolare di input opzionale, validata a partire da configurazione esterna
/// (mai un panic: cfr. `PLAN.md` §2 principio 4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputArea {
    x_min: Option<f32>,
    x_max: Option<f32>,
    y_min: Option<f32>,
    y_max: Option<f32>,
}

fn validate_positive(v: Option<f32>, err: impl Fn(f32) -> PositionError) -> Result<(), PositionError> {
    match v {
        Some(v) if v <= 0.0 => Err(err(v)),
        _ => Ok(()),
    }
}

impl InputArea {
    pub fn build(x_min: Option<f32>, x_max: Option<f32>, y_min: Option<f32>, y_max: Option<f32>) -> Result<Self, PositionError> {
        validate_positive(x_min, PositionError::XMinNotPositive)?;
        validate_positive(x_max, PositionError::XMaxNotPositive)?;
        validate_positive(y_min, PositionError::YMinNotPositive)?;
        validate_positive(y_max, PositionError::YMaxNotPositive)?;
        if let (Some(mn), Some(mx)) = (x_min, x_max)
            && mx <= mn
        {
            return Err(PositionError::XBoundsInverted { x_min: mn, x_max: mx });
        }
        if let (Some(mn), Some(mx)) = (y_min, y_max)
            && mx <= mn
        {
            return Err(PositionError::YBoundsInverted { y_min: mn, y_max: mx });
        }
        Ok(Self { x_min, x_max, y_min, y_max })
    }

    pub fn x_min(&self) -> Option<f32> {
        self.x_min
    }
    pub fn x_max(&self) -> Option<f32> {
        self.x_max
    }
    pub fn y_min(&self) -> Option<f32> {
        self.y_min
    }
    pub fn y_max(&self) -> Option<f32> {
        self.y_max
    }
}

/// Raggruppa `lines` per prossimita' lungo un asse. L'ordine del risultato segue l'ordine
/// *ordinato* delle chiavi, non l'ordine di `lines` in ingresso: comportamento del riferimento
/// preservato deliberatamente (`PLAN.md` §0).
pub fn get_groups(lines: &[PdfLine], threshold: f32, vertical: bool) -> Result<Vec<i64>, PositionError> {
    if lines.is_empty() {
        return Err(PositionError::EmptyLines);
    }
    let geoindex = if vertical { 1 } else { 0 };
    let mut keys: Vec<f32> = lines
        .iter()
        .map(|l| {
            let bbox = l.bbox().as_tuple();
            [bbox.0, bbox.1, bbox.2, bbox.3][geoindex]
        })
        .collect();
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut groups = Vec::with_capacity(keys.len());
    let mut group_id: i64 = 0;
    let mut a = keys[0];
    for b in keys {
        if (b - a).abs() >= threshold {
            group_id += 1;
        }
        a = b;
        groups.push(group_id);
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::geometry::Limits;
    use crate::formats_utils::pdf_extract::tabularizer::collapse::{SplittingDirection, SplittingState};

    mod row_column_table_config {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn row_config_just_wraps_optional_limits() {
            let none = RowConfig { limits: None };
            let some = RowConfig { limits: Some(Limits::new(1.0, 2.0)) };
            assert_eq!(none.limits, None);
            assert_eq!(some.limits.unwrap().as_tuple(), (1.0, 2.0));
        }

        #[test]
        fn column_config_holds_limits_splitting_and_nullable_independently() {
            let col = ColumnConfig { limits: Some(Limits::new(0.0, 10.0)), splitting: Some(SplittingState::Allow(SplittingDirection::Up)), nullable: Some(true) };
            assert_eq!(col.limits.unwrap().as_tuple(), (0.0, 10.0));
            assert_eq!(col.splitting, Some(SplittingState::Allow(SplittingDirection::Up)));
            assert_eq!(col.nullable, Some(true));
        }

        #[test]
        fn table_config_holds_independent_column_configs_not_aliased_clones() {
            let base = ColumnConfig { limits: None, splitting: None, nullable: None };
            let mut cols = vec![base; 3];
            cols[1].splitting = Some(SplittingState::Disallow);
            let table = TableConfig { cols: Some(cols), rows: None };
            let cols = table.cols.unwrap();
            assert_eq!(cols[0].splitting, None);
            assert_eq!(cols[1].splitting, Some(SplittingState::Disallow));
            assert_eq!(cols[2].splitting, None);
        }
    }

    mod input_area_construction {
        use super::*;

        #[test]
        fn accepts_all_bounds_present_and_correctly_ordered() {
            let area = InputArea::build(Some(1.0), Some(10.0), Some(2.0), Some(20.0)).unwrap();
            assert_eq!(area.x_min(), Some(1.0));
            assert_eq!(area.x_max(), Some(10.0));
            assert_eq!(area.y_min(), Some(2.0));
            assert_eq!(area.y_max(), Some(20.0));
        }

        #[test]
        fn accepts_all_bounds_missing() {
            let area = InputArea::build(None, None, None, None).unwrap();
            assert_eq!((area.x_min(), area.x_max(), area.y_min(), area.y_max()), (None, None, None, None));
        }

        #[test]
        fn accepts_a_single_bound_with_no_partner_to_compare_against() {
            let area = InputArea::build(Some(5.0), None, None, None).unwrap();
            assert_eq!(area.x_min(), Some(5.0));
        }

        // Nota: i confronti sotto usano `if let` + `assert_eq!` sui campi invece di
        // `matches!(.., Variant(0.0))`: un pattern letterale in virgola mobile e' rifiutato dal
        // compilatore (`illegal_floating_point_literal_pattern`), quindi non e' disponibile qui.

        #[test]
        fn rejects_non_positive_x_min() {
            let err = InputArea::build(Some(0.0), None, None, None).unwrap_err();
            let PositionError::XMinNotPositive(v) = err else { panic!("expected XMinNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_negative_x_min() {
            let err = InputArea::build(Some(-1.0), None, None, None).unwrap_err();
            let PositionError::XMinNotPositive(v) = err else { panic!("expected XMinNotPositive, got {err:?}") };
            assert_eq!(v, -1.0);
        }

        #[test]
        fn rejects_non_positive_x_max() {
            let err = InputArea::build(None, Some(0.0), None, None).unwrap_err();
            let PositionError::XMaxNotPositive(v) = err else { panic!("expected XMaxNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_non_positive_y_min() {
            let err = InputArea::build(None, None, Some(0.0), None).unwrap_err();
            let PositionError::YMinNotPositive(v) = err else { panic!("expected YMinNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_non_positive_y_max() {
            let err = InputArea::build(None, None, None, Some(0.0)).unwrap_err();
            let PositionError::YMaxNotPositive(v) = err else { panic!("expected YMaxNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_x_max_not_greater_than_x_min() {
            let err = InputArea::build(Some(10.0), Some(5.0), None, None).unwrap_err();
            let PositionError::XBoundsInverted { x_min, x_max } = err else { panic!("expected XBoundsInverted, got {err:?}") };
            assert_eq!((x_min, x_max), (10.0, 5.0));
        }

        #[test]
        fn rejects_x_max_equal_to_x_min() {
            let err = InputArea::build(Some(10.0), Some(10.0), None, None).unwrap_err();
            let PositionError::XBoundsInverted { x_min, x_max } = err else { panic!("expected XBoundsInverted, got {err:?}") };
            assert_eq!((x_min, x_max), (10.0, 10.0));
        }

        #[test]
        fn rejects_y_max_not_greater_than_y_min() {
            let err = InputArea::build(None, None, Some(10.0), Some(5.0)).unwrap_err();
            let PositionError::YBoundsInverted { y_min, y_max } = err else { panic!("expected YBoundsInverted, got {err:?}") };
            assert_eq!((y_min, y_max), (10.0, 5.0));
        }
    }

    mod get_groups_behavior {
        use super::*;

        fn line_at(x: f32, y: f32) -> PdfLine {
            PdfLine::new("Arial", 10.0, "x", (x, y, x + 1.0, y + 1.0))
        }

        #[test]
        fn splits_on_threshold_along_the_vertical_axis_by_default() {
            let lines = vec![line_at(0.0, 0.0), line_at(0.0, 1.0), line_at(0.0, 10.0)];
            assert_eq!(get_groups(&lines, 5.0, true).unwrap(), vec![0, 0, 1]);
        }

        #[test]
        fn uses_the_horizontal_axis_when_not_vertical() {
            let lines = vec![line_at(0.0, 0.0), line_at(10.0, 0.0)];
            assert_eq!(get_groups(&lines, 5.0, false).unwrap(), vec![0, 1]);
        }

        #[test]
        fn returns_group_ids_in_sorted_key_order_not_in_input_order() {
            // Ordine di input: y = 10, 0, 5 (differenze successive nell'ordine ordinato:
            // 0->5 = 5, 5->10 = 5, entrambe >= soglia): il riferimento restituisce gli id nel
            // *stesso ordine delle chiavi ordinate* (0,1,2), non nell'ordine delle righe in
            // ingresso — comportamento preservato deliberatamente, non e' un bug da correggere.
            let lines = vec![line_at(0.0, 10.0), line_at(0.0, 0.0), line_at(0.0, 5.0)];
            assert_eq!(get_groups(&lines, 3.0, true).unwrap(), vec![0, 1, 2]);
        }

        #[test]
        fn keeps_close_values_in_the_same_group() {
            let lines = vec![line_at(0.0, 0.0), line_at(0.0, 1.0), line_at(0.0, 1.5)];
            assert_eq!(get_groups(&lines, 5.0, true).unwrap(), vec![0, 0, 0]);
        }

        #[test]
        fn errors_on_an_empty_list_of_lines() {
            let lines: Vec<PdfLine> = vec![];
            assert!(matches!(get_groups(&lines, 1.0, true), Err(PositionError::EmptyLines)));
        }
    }
}
