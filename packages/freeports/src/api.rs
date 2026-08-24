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
//! `ColumnConfig`. M5 aggiunge `Pipeline` e
//! `Algorithm`, con tutto ciò che serve a costruirli e a leggerne i risultati (vedi il doc-comment
//! di `core`).
//!
//! M6 chiude il sottoinsieme di `utils::pdf_extract` lasciato aperto da M3 (`agent-memory/
//! M6-implementation-plan.md`): le quattro funzioni di `PLAN.md` §9 che leggono un dict PyMuPDF o
//! una stringa di configurazione (`pdfline_selection_from_dict`, `pdfline_selection_from_str`,
//! `pdfimages_from_pagedict`, `pdflines_from_pagedict`), più i tipi di supporto senza i quali non
//! sono costruibili/leggibili da fuori (`PageDict` e le sue tre parti; la forma "dict" di criterio
//! `InputPdfLineSet`/`FontCriterion`/`InputAreaSpec`; `LineSelectionError`) — stesso trattamento
//! già riservato a `BlockType`/`BlockValue` in M2. M6 aggiunge anche un nuovo modulo `api::input`,
//! non elencato da `PLAN.md` §9 (che per `input` nomina solo `load_target_companies`/
//! `compile_target_companies`, `input::companies_db`, fuori scope qui): senza `load_document`/
//! `load_document_pages` nessun consumatore esterno potrebbe mai costruire un `Document` reale da
//! un path PDF, quindi il buco fra §9 e lo scope necessario è documentato allo stesso modo,
//! non lasciato silenzioso (M6, Q2 confermata dall'utente).
//!
//! M7 chiude il resto: gli otto pipe `standard_funcs::pdf_extract` (D-M7-1), i tre livelli del
//! repo formati con `Algorithm::load` che li fonde, e **`TablePosMeasureUnit`**, che risolve
//! `PLAN.md` §13 punto 4 — il tipo esisteva nel riferimento, in `position.py`, come unità di
//! misura della tolleranza del `get_table_coordinates` che parte dalle *righe* di una pagina.
//! Quel wrapper è ciò che §9 elenca e ciò che gli autori di formato chiamano davvero, quindi è lui
//! a portare qui il nome `get_table_coordinates`; la funzione per celle già costruite di
//! `tabularizer::coordinates`, esportata da M3 con quel nome, diventa
//! `get_table_coordinates_from_cells`. È l'unica rinomina di questa superficie finora, e riguarda
//! solo l'alias pubblico: dentro il crate i due nomi restano quelli di sempre.

pub mod consts {
    pub use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
}

pub mod core {
    //! `PLAN.md` §9 elenca `PdfBlock`, `TextBlock` e `Promise`; a questi si aggiungono
    //! `BlockType` e `BlockValue`, che l'elenco non nominava ma senza i quali i due blocchi sono
    //! inutilizzabili — sono i tipi dei loro campi pubblici. `BlockValueError` viaggia con
    //! `BlockValue` per la stessa ragione: e' l'errore che restituiscono i suoi accessori.
    //!
    //! M5 aggiunge i due tipi che `PLAN.md` §9 elencava e che non esistevano ancora, `Pipeline` e
    //! `Algorithm`, e con loro il resto del motore. Come già per `BlockType`/`BlockValue` in M2,
    //! l'elenco di §9 nomina solo le due facciate: da fuori il crate non si può costruire un
    //! `Algorithm` senza i tipi dei suoi argomenti (`PipelineName`, `Schedule`, `PageClass`,
    //! `PageClassFinalizer`), non si può scrivere un pipe senza il suo trait e senza
    //! `FilterData`/`Extracted`/`PipeError`, e non si possono leggere i risultati senza
    //! `DocumentOutcome`/`PageOutcome`. Sono tutti tipi di firme pubbliche già esposte, non
    //! superficie nuova per scelta.
    pub use crate::core::algorithm::{
        Algorithm, AlgorithmError, DocumentOutcome, PageClassFinalize, PageClassFinalizer,
        PageOutcome,
    };
    pub use crate::core::classes::{BlockType, BlockValue, BlockValueError, PdfBlock, TextBlock};
    pub use crate::core::page::{Document, DocumentId, FormatName, Page, PageError, PageImage};
    pub use crate::core::pipeline::bundle::PipelinesBundle;
    pub use crate::core::pipeline::{
        DeserializePipe, DeserializeSegment, Extracted, FilterData, PdfExtractPipe,
        PdfExtractSegment, PipeError, Pipeline, PipelineName, PromiseEntries, Segment,
        TextFilterPipe, TextFilterSegment,
    };
    pub use crate::core::promise::{Promise, PromiseError};
    pub use crate::core::promise_resolution::{FlatPromiseMap, PromiseMap};
    pub use crate::core::schedule::{PageClass, Schedule, ScheduleError, ScheduleStep, ScheduledPage};
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
            CellGeometry, CoordinateExtractionError, TablePosAlgorithm,
            get_table_coordinates as get_table_coordinates_from_cells,
        };
        pub use crate::formats_utils::pdf_extract::tabularizer::{
            TableCoordinatesConfig, TablePosMeasureUnit,
            get_table_coordinates_from_lines as get_table_coordinates,
        };
        pub use crate::input::document::page_dict::{
            PageDict, PageDictBlock, PageDictLine, PageDictSpan, pdfimages_from_pagedict,
            pdflines_from_pagedict,
        };
        pub use crate::input::document::selection::{
            FontCriterion, InputAreaSpec, InputPdfLineSet, LineSelectionError,
            pdfline_selection_from_dict, pdfline_selection_from_str,
        };
    }
}

pub mod input {
    //! Non elencato da `PLAN.md` §9 (che per `input` nomina solo `load_target_companies`/
    //! `compile_target_companies`, `input::companies_db`, fuori scope qui) — gap fra §9 e lo
    //! scope necessario, stesso trattamento di `TablePosMeasureUnit` (vedi `STATUS.md`). Senza un
    //! punto d'ingresso che apra un PDF reale, nessun consumatore fuori da questo crate potrebbe
    //! mai costruire un `Document` (M6, Q2 confermata dall'utente,
    //! `agent-memory/M6-implementation-plan.md`).
    pub use crate::input::document::{DocumentError, load_document, load_document_pages};
}

pub mod standard_funcs {
    //! I pipe pronti che i repo formati costruiscono dai propri file di configurazione.
    //!
    //! `PLAN.md` §9 elenca tre famiglie; M7 abilita quella `pdf_extract` (decisione D-M7-1) e la
    //! parte già esistente di `text_filter`/`deserialize` — le dieci funzioni che costruiscono
    //! entità di `output::classes` arrivano con M8, tranne le tre anticipate da D-M7-2.
    //!
    //! Le tre "factory" del riferimento (`PdfExtractFundStandard` e sorelle) sono funzioni e non
    //! tipi, perché nel riferimento non sono mai state altro che tre costruttori sopra lo stesso
    //! tipo, `ExtractTextPdfBlockOrFailPage`.
    pub mod pdf_extract {
        pub use crate::formats_utils::pdf_extract::standard_funcs::{
            AssetsColumn, AssetsStandardArgs, ExtractTextPdfBlockOrFailPage, InvestmentsStandardArgs,
            PdfExtractAssetsStandard, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
            PdfExtractPageClassifyStandard, PdfExtractSfdrArticleStandard, PdfExtractStandardFuncsError,
            pdf_extract_currency_standard, pdf_extract_fund_standard, pdf_extract_managment_company_standard,
        };
    }

    pub mod text_filter {
        //! M8 aggiunge le tre funzioni restanti (`TextFilterSfdrArticleStandard`,
        //! `TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`), che dipendevano da
        //! `output::classes`.
        pub use crate::formats_utils::text_filter::standard_funcs::{
            StandardFuncsError, TextFilterAssetsStandard, TextFilterInvestmentsStandard,
            TextFilterManagmentCompanyStandard, TextFilterPageClassifyStandard, TextFilterSfdrArticleStandard,
        };
    }

    pub mod deserialize {
        //! M8 aggiunge le cinque funzioni restanti (`DeserializeSfdrArticleStandard`,
        //! `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
        //! `DeserializerInvestmentsManagerStandard`, `DeserializerAssetsStandard`), che dipendevano
        //! da `output::classes` — chiude anche M4.
        pub use crate::formats_utils::deserialize::standard_funcs::{
            DeserializeSfdrArticleStandard, DeserializeStandardFuncsError, DeserializerAssetsStandard,
            DeserializerFundStandard, DeserializerInvestmentStandard, DeserializerInvestmentsManagerFromManco,
            DeserializerInvestmentsManagerStandard, DeserializerManagmentCompanyStandard,
            DeserializerPageClassifyStandard,
        };
    }
}

pub mod formats_repo {
    //! Il caricamento di un repo formati.
    //!
    //! `PLAN.md` §9 non elenca questo modulo, perché la sua unica facciata è `Algorithm::load`,
    //! che vive già su `core::Algorithm`. Sono riesportati comunque il tipo d'errore — senza il
    //! quale `load` non è gestibile da fuori — e le due funzioni di riconoscimento del formato da
    //! URL, che sono ciò che un chiamante usa *prima* di sapere quale formato caricare.
    pub use crate::formats_repo::LoadError;
    pub use crate::formats_repo::load_pipelines;
    pub use crate::formats_repo::metadata::{MetadataError, get_formats, get_url_mapping, url_to_format};
}

pub mod output {
    //! Le entità prodotte dai pipe `deserialize`.
    //!
    //! **Completo da M8.** `PLAN.md` §9 elenca questa superficie con nomi che non corrispondono
    //! a quelli reali del codice (`FundChangeName` singolare invece di `FundRename`/`FundMerge`;
    //! `SfdrArticle` invece di `FundSfdrClassification`, che collide col nome già pubblico di
    //! `consts::SfdrArticle`; `FundEsgIndicators` plurale invece di `FundEsgIndicator` singolare)
    //! — riesportati qui sono i nomi **reali** già scelti nel codice, non la lettera di §9
    //! (`PLAN.md` §13, decisione Q2, stessa filosofia già usata per `get_table_coordinates`/
    //! `TablePosMeasureUnit` in M7). M7 aveva anticipato `Fund`/`Equity`/`Bond` (decisione D-M7-2).
    pub use crate::output::classes::assets_manager::{AssetsManagerData, InvestmentsManager, ManagementCompany};
    pub use crate::output::classes::fund::Fund;
    pub use crate::output::classes::fund_assets::FundAssets;
    pub use crate::output::classes::fund_change_name::{FundChangeNameData, FundMerge, FundRename};
    pub use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
    pub use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
    pub use crate::output::classes::investment::{Bond, Equity, InvestmentData, InvestmentFields};
    pub use crate::output::classes::{FloatConstraint, OutputClassError};
}
