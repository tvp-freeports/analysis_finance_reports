//! The crate's public surface: re-exports only, no logic of its own.
//!
//! Everything reachable from `crate::api` is a promise to library users; everything else is an
//! internal tree that may be reorganised at any time. The two are kept apart on purpose, so that
//! moving a type between modules is never a breaking change.
//!
//! The surface is deliberately narrower than the internal tree, and each submodule below explains
//! what it exposes and why. One rule recurs: a facade type drags in the types of its own public
//! fields and signatures. Exposing [`core::PdfBlock`] without `BlockType` and
//! `BlockValue` would give callers a struct they cannot read; exposing a fallible function without
//! its error type would give them a `Result` they cannot match on. Those companions are not surface
//! creep, they are what makes the facade usable.
//!
//! A few public names differ from their internal ones, where the internal name is right inside the
//! crate but ambiguous outside it — see [`utils::pdf_extract`] for the only two cases.

pub mod consts {
    //! The closed vocabularies an extracted value can take: currency, instrument kind, SFDR
    //! article.
    pub use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
}

pub mod core {
    //! The extraction engine: documents, pages, pipelines, promises, and the algorithm that runs
    //! them.
    //!
    //! [`Algorithm`] is the entry point — it turns a [`Document`] into [`DocumentOutcome`]s — and
    //! the rest of this module is what a caller needs to build one, to write a pipe for it, or to
    //! read its results. [`Parallelism`] is the argument of the `*_with` variants of those methods;
    //! without it only the sequential signatures are reachable from outside the crate.
    pub use crate::core::algorithm::{
        Algorithm, AlgorithmError, DocumentOutcome, PageClassFinalize, PageClassFinalizer,
        PageOutcome,
    };
    pub use crate::core::classes::{BlockType, BlockValue, BlockValueError, PdfBlock, TextBlock};
    pub use crate::core::page::{Document, DocumentId, FormatName, Page, PageError, PageImage};
    pub use crate::core::parallelism::Parallelism;
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
    //! Geometry helpers a format author reaches for while writing a `pdf_extract` pipe.
    pub mod pdf_extract {
        //! Positioning and tabularisation: turning the blocks of a page into rows, columns and
        //! tables.
        //!
        //! Two names differ from their internal ones. Inside the crate,
        //! `coordinates::get_table_coordinates` takes cells that are already built and
        //! `tabularizer::get_table_coordinates_from_lines` starts from the lines of a page; from
        //! outside, the second is the one format authors actually call, so it takes the plain name
        //! here and the first becomes `get_table_coordinates_from_cells`.
        //!
        //! `Limits` belongs to `commons::geometry` rather than here, but it is the type of the
        //! `limits` field of both [`RowConfig`] and [`ColumnConfig`]: without it neither struct can
        //! be built.
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
    //! Opening a PDF and reading its pages.
    //!
    //! The engine consumes [`Document`](super::core::Document) values, and this is the only way to
    //! obtain a real one from a path — everything else in the surface assumes the document already
    //! exists.
    pub use crate::input::document::{DocumentError, load_document, load_document_pages};
}

pub mod standard_funcs {
    //! The ready-made pipes that a formats repository builds from its own configuration files.
    //!
    //! These are the three pipe families of the engine — `pdf_extract`, `text_filter`,
    //! `deserialize` — in the form a format author selects by name instead of implementing. Writing
    //! a format means choosing among these and configuring them; writing a *new* pipe means
    //! implementing the corresponding trait from [`super::core`] instead.
    //!
    //! Some of them are plain functions rather than types: where a family needs several
    //! constructors over one underlying pipe, the constructors are functions and the pipe stays a
    //! single type.
    pub mod pdf_extract {
        //! Pipes that turn a raw page into blocks: text, tables, page classification.
        pub use crate::formats_utils::pdf_extract::standard_funcs::{
            AssetsColumn, AssetsStandardArgs, ExtractTextPdfBlockOrFailPage, InvestmentsStandardArgs,
            PdfExtractAssetsStandard, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
            PdfExtractPageClassifyStandard, PdfExtractSfdrArticleStandard, PdfExtractStandardFuncsError,
            pdf_extract_currency_standard, pdf_extract_fund_standard, pdf_extract_managment_company_standard,
        };
    }

    pub mod text_filter {
        //! Pipes that keep only the blocks belonging to the funds and fields being looked for.
        pub use crate::formats_utils::text_filter::standard_funcs::{
            StandardFuncsError, TextFilterAssetsStandard, TextFilterInvestmentsStandard,
            TextFilterManagmentCompanyStandard, TextFilterPageClassifyStandard, TextFilterSfdrArticleStandard,
        };
    }

    pub mod deserialize {
        //! Pipes that turn surviving blocks into the typed entities of [`super::super::output`].
        pub use crate::formats_utils::deserialize::standard_funcs::{
            DeserializeSfdrArticleStandard, DeserializeStandardFuncsError, DeserializerAssetsStandard,
            DeserializerFundStandard, DeserializerInvestmentStandard, DeserializerInvestmentsManagerFromManco,
            DeserializerInvestmentsManagerStandard, DeserializerManagmentCompanyStandard,
            DeserializerPageClassifyStandard,
        };
    }
}

pub mod formats_repo {
    //! Loading a formats repository.
    //!
    //! The main facade is `Algorithm::load`, which lives on [`Algorithm`](super::core::Algorithm);
    //! exported here are its error type and the two format-recognition helpers, which a caller uses
    //! *before* it knows which format to load — mapping a document URL to a format name.
    pub use crate::formats_repo::LoadError;
    pub use crate::formats_repo::load_pipelines;
    pub use crate::formats_repo::metadata::{MetadataError, get_formats, get_url_mapping, url_to_format};
}

pub mod output {
    //! The entities produced by `deserialize` pipes: what an extraction run ultimately yields.
    //!
    //! One name deserves an explanation: `FundSfdrClassification` is the fund-level entity,
    //! distinct from [`consts::SfdrArticle`](super::consts::SfdrArticle), which is the enumerated
    //! article value carried inside it.
    pub use crate::output::classes::assets_manager::{AssetsManagerData, InvestmentsManager, ManagementCompany};
    pub use crate::output::classes::fund::Fund;
    pub use crate::output::classes::fund_assets::FundAssets;
    pub use crate::output::classes::fund_change_name::{FundChangeNameData, FundMerge, FundRename};
    pub use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
    pub use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
    pub use crate::output::classes::investment::{Bond, Equity, InvestmentData, InvestmentFields};
    pub use crate::output::classes::{FloatConstraint, OutputClassError};
}

pub mod cli {
    //! Driving the command-line application from code.
    //!
    //! [`CliArgs`] plus [`execute`] are what `main` itself uses, exposed so that an embedding
    //! program or an integration test can run a whole job without shelling out to the binary.
    pub use crate::cli::config_locations::cmd::CliArgs;
    pub use crate::cli::run::{CliError, execute};
}
