//! Superficie pubblica del crate (`PLAN.md` §9): sole re-export, nessuna logica propria.
//! L'albero interno resta libero di cambiare; solo ciò che è re-esportato da qui è garantito
//! per chi usa la libreria.
//!
//! Abilitato incrementalmente, milestone per milestone, man mano che i moduli esistono davvero
//! (`PLAN.md` §14, passo di chiusura). M1 abilita solo `consts`, l'unica porzione di `commons`
//! elencata nella superficie pubblica di `PLAN.md` §9 — `date`/`geometry`/`sets`/`flag_expr`/
//! `i18n` restano utilità interne, usate da milestone future ma non riesportate.

pub mod consts {
    pub use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
}
