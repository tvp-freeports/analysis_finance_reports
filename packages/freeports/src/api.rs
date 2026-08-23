//! Superficie pubblica del crate (`PLAN.md` §9): sole re-export, nessuna logica propria.
//! L'albero interno resta libero di cambiare; solo ciò che è re-esportato da qui è garantito
//! per chi usa la libreria.
//!
//! Abilitato incrementalmente, milestone per milestone, man mano che i moduli esistono davvero
//! (`PLAN.md` §14, passo di chiusura). M1 abilita solo `consts`, l'unica porzione di `commons`
//! elencata nella superficie pubblica di `PLAN.md` §9 — `date`/`geometry`/`sets`/`flag_expr`/
//! `i18n` restano utilità interne, usate da milestone future ma non riesportate. M2 aggiunge la
//! parte di `core` che esiste: `PdfBlock`, `TextBlock` e `Promise` (`Pipeline` e `Algorithm`
//! arrivano con M5). M3 aggiunge la parte di `utils::pdf_extract` che non dipende dal confine
//! PyMuPDF (arriva con M6): `position`/`tabularizer` e le config `TableConfig`/`RowConfig`/
//! `ColumnConfig`. `pdfline_selection_from_dict`, `pdfline_selection_from_str`,
//! `pdfimages_from_pagedict`, `pdflines_from_pagedict` (`PLAN.md` §9) restano non riesportati:
//! costruiscono `PdfLine`/`PdfLineSelection` da un dict PyMuPDF o costruiscono selezioni da
//! stringa, e appartengono a `input::document` (M6). `TablePosMeasureUnit`, anch'esso elencato in
//! §9, non ha né riferimento né test nel milestone M3: e' un buco fra §9 e lo scope reale di M3,
//! annotato in `STATUS.md`, non un'omissione silenziosa.

pub mod consts {
    pub use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
}

pub mod core {
    //! `PLAN.md` §9 elenca `PdfBlock`, `TextBlock` e `Promise`; a questi si aggiungono
    //! `BlockType` e `BlockValue`, che l'elenco non nominava ma senza i quali i due blocchi sono
    //! inutilizzabili — sono i tipi dei loro campi pubblici. `BlockValueError` viaggia con
    //! `BlockValue` per la stessa ragione: e' l'errore che restituiscono i suoi accessori.
    pub use crate::core::classes::{BlockType, BlockValue, BlockValueError, PdfBlock, TextBlock};
    pub use crate::core::promise::{Promise, PromiseError};
}

pub mod utils {
    pub mod pdf_extract {
        //! Sottoinsieme di `PLAN.md` §9 già disponibile a fine M3: le config di
        //! posizionamento/tabularizzazione e i loro tipi di supporto. `Limits` viene da
        //! `commons::geometry` (M1) ma è qui perché è il tipo dei campi `limits` di
        //! `RowConfig`/`ColumnConfig` — senza, quelle due struct non sono costruibili da fuori.
        pub use crate::commons::geometry::Limits;
        pub use crate::formats_utils::pdf_extract::position::{
            ColumnConfig, PositionError, RowConfig, TableConfig, get_groups,
        };
        pub use crate::formats_utils::pdf_extract::tabularizer::collapse::{
            CollapseAlgorithm, NullableState, SplittingState,
        };
        pub use crate::formats_utils::pdf_extract::tabularizer::coordinates::{
            CellGeometry, CoordinateExtractionError, TablePosAlgorithm, get_table_coordinates,
        };
    }
}
