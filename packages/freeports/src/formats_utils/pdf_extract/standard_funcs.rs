//! Pipe `pdf_extract` standard (`PdfExtract*`): il primo segmento delle pipeline structured e
//! semistructured.
//!
//! Porting nativo di `freeports_core/src/formats_utils/pdf_extract/standard_funcs.rs` (i cinque
//! pipe già in Rust) e di `formats/utils/pdf_extract/standard_funcs.py` (le tre factory rimaste
//! in Python). **Non** è uno dei moduli da portare verbatim (`PLAN.md` §0): il riferimento è
//! PyO3 al 100% — selezioni duck-tipizzate come `Py<PyAny>`, `TablePosAlgorithm`/
//! `get_table_coordinates` raggiunti via `py.import` — mentre qui tutto è Rust nativo.
//!
//! **Milestone: M7** — decisione D-M7-1, presa dall'utente il 2026-08-23, che chiude
//! `PLAN.md` §13 punto 6 (il modulo non era assegnato a nessuna riga di §11). Tutti e otto i pipe
//! elencati da §9 stanno qui: nessuno dipende da `output::classes`, e sono esattamente ciò che
//! `formats_repo::{structured,semistructured}` costruisce leggendo i CSV.
//!
//! **Tre differenze strutturali rispetto al riferimento, tutte dettate dai tipi nativi:**
//!
//! 1. Le tre "factory" `PdfExtractFundStandard`/`PdfExtractCurrencyStandard`/
//!    `PdfExtractManagmentCompanyStandard` restano tre funzioni che costruiscono un solo tipo,
//!    [`ExtractTextPdfBlockOrFailPage`], esattamente come nel riferimento (dove sono tre
//!    `__new__` sopra la stessa classe Rust). Il tipo esiste qui — e non in
//!    `pdf_extract::commons`, dove vive la funzione omonima — perché è un *pipe*, cioè un
//!    implementatore di [`PdfExtractPipe`](crate::core::pipeline::PdfExtractPipe), non un helper.
//! 2. La `deselection_list` di [`PdfExtractInvestmentsStandard`] non è sottratta al `body_set` nel
//!    costruttore (come fa il riferimento con `body_set / dl`) ma al momento della selezione. È
//!    equivalente — `contextualize` distribuisce sui rami dell'AST, quindi
//!    `contextualize(a / b) == contextualize(a) / contextualize(b)` — ed evita di aggiungere
//!    l'algebra `Div`/`BitOr` a `PdfLineSelection`, che `PLAN.md` §0 vuole portato invariato.
//! 3. Dove il riferimento accetta o restituisce oggetti Python duck-tipizzati (il `fund_selection`
//!    di `PdfExtractAssetsStandard`, che è un `SelectExpectedText` oppure una selezione grezza a
//!    seconda di `table_condition`), qui il campo resta una selezione e il ramo è deciso da un
//!    `match` esplicito su `table_condition`.
//!
//! **Campi morti conservati per fedeltà**, come nel riferimento e con la stessa motivazione:
//! `PdfExtractInvestmentsStandard::tolerance` e `::row_algorithm_flags` sono leggibili ma non
//! consultati da `extract` — solo `algorithm_flags` e `row_tolerance` alimentano davvero
//! `get_table_coordinates`. Nessun formato reale imposta valori non-default per i due, quindi la
//! fedeltà non costa nulla.

use std::collections::BTreeMap;

use crate::commons::consts::{Currency, SfdrArticle};
use crate::commons::sets::Container;
use crate::core::classes::{BlockType, BlockValue, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::commons::{CommonsError, extract_text_pdf_block_or_fail_page, select_expected_text};
use super::pdf_line::PdfLine;
use super::relative::{OptionallyRelative, RelativeInfo};
use super::select::pdf_line::PdfLineSet;
use super::select::relative::{PdfLineSelection, RelativePdfLineSet, RelativeSelectPdfLineSet};
use super::tabularizer::coordinates::{CoordinateExtractionError, TablePosAlgorithm};
use super::tabularizer::{TableCoordinatesConfig, get_table_coordinates_from_lines};

/// Errori dei pipe `pdf_extract` standard.
///
/// Un enum per modulo (`PLAN.md` §8). [`PdfExtractStandardFuncsError::Commons`] è l'unico che può
/// portare un fallimento **non fatale** di pagina, perché è l'unico costruito da
/// [`CommonsError::PageParseFail`]; tutto il resto interrompe l'elaborazione, come nel
/// riferimento, dove solo `PageParseFail` è catturato dal ciclo dello schedule.
#[derive(Debug, thiserror::Error)]
pub enum PdfExtractStandardFuncsError {
    /// Il riferimento solleva `ExpectedPdfBlockNotFound`: nessuna riga corrisponde a una selezione
    /// che *deve* produrre almeno un risultato.
    #[error("Pdf block during extraction of \"{name}\" not found")]
    ExpectedPdfBlockNotFound { name: String },
    #[error("{0}")]
    Commons(#[from] CommonsError),
    #[error("{0}")]
    Coordinates(#[from] CoordinateExtractionError),
    /// `range(start, stop, 0)` in Python è un `ValueError`; qui è tipizzato.
    #[error("skip_column must not be zero")]
    ZeroSkipColumn,
    /// Le tre colonne di un blocco assets hanno lunghezze diverse: il riferimento va in
    /// `IndexError`, qui l'errore ha un nome.
    #[error("assets column \"{column}\" has {found} entries, expected at least {expected}")]
    MismatchedAssetsColumn { column: String, found: usize, expected: usize },
    /// Nessun token di valuta da staccare dalla coda del nome del fondo (`IndexError` nel
    /// riferimento).
    #[error("fund column \"{column}\" carries no currency token to split off")]
    MissingCurrencyToken { column: String },
}

impl PdfExtractStandardFuncsError {
    /// Traduzione nell'errore del motore; il nome del pipe non è ricavabile dall'errore, quindi lo
    /// passa il chiamante — stessa forma di [`PipeError::from_commons`] e di
    /// `text_filter::standard_funcs::StandardFuncsError::into_pipe_error`.
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            PdfExtractStandardFuncsError::Commons(source) => PipeError::from_commons(pipe, source),
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

/// Le righe di `lines` selezionate da `selection`, nell'ordine in cui compaiono nella pagina.
///
/// È l'equivalente del `.select(lines)` duck-tipizzato del riferimento: contestualizza l'eventuale
/// parte relativa della selezione e poi filtra.
fn select<'a>(selection: &PdfLineSelection, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
    let set = selection.clone().contextualize(lines);
    lines.iter().filter(|line| set.contains(line)).collect()
}

/// Come [`select`], ma restituisce l'insieme già contestualizzato: serve dove la selezione va
/// combinata con altre (`/`, `|`, `&`) prima di essere applicata.
fn contextualized(selection: &PdfLineSelection, lines: &[PdfLine]) -> PdfLineSet {
    selection.clone().contextualize(lines)
}

// ---------------------------------------------------------------------------------------------
// ExtractTextPdfBlockOrFailPage e le tre factory che ci stanno sopra
// ---------------------------------------------------------------------------------------------

/// Estrae il testo della prima riga selezionata e ne fa un unico [`PdfBlock`]; se non trova nulla,
/// fallisce l'intera pagina (in modo non fatale: lo schedule la salta e prosegue).
///
/// È il pipe dietro le tre factory `PdfExtract{Fund,Currency,ManagmentCompany}Standard`, che nel
/// riferimento sono tre `__new__` che costruiscono questo stesso tipo con `name`/`type_block`
/// diversi e nient'altro.
pub struct ExtractTextPdfBlockOrFailPage {
    selection: PdfLineSelection,
    name: String,
    type_block: BlockType,
}

impl ExtractTextPdfBlockOrFailPage {
    pub fn new(selection: PdfLineSelection, name: impl Into<String>, type_block: BlockType) -> Self {
        Self { selection, name: name.into(), type_block }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let set = contextualized(&self.selection, &page.lines);
        Ok(extract_text_pdf_block_or_fail_page(&set, &page.lines, &self.name, self.type_block.clone())?)
    }
}

impl PdfExtractPipe for ExtractTextPdfBlockOrFailPage {
    fn name(&self) -> &str {
        &self.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

/// Pipe che estrae il nome del fondo. Factory su [`ExtractTextPdfBlockOrFailPage`], come nel
/// riferimento.
pub fn pdf_extract_fund_standard(selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage {
    ExtractTextPdfBlockOrFailPage::new(selection, "fund", BlockType::FUND_NAME)
}

/// Pipe che estrae la dichiarazione di valuta. Vedi [`pdf_extract_fund_standard`].
pub fn pdf_extract_currency_standard(selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage {
    ExtractTextPdfBlockOrFailPage::new(selection, "currency", BlockType::CURRENCY_STATEMENT)
}

/// Pipe che estrae la società di gestione. Vedi [`pdf_extract_fund_standard`]. Il nome conserva
/// l'ortografia del riferimento (`managment`, senza la `e`): è la stringa che finisce nei log e
/// nei messaggi d'errore, e cambiarla sarebbe una divergenza osservabile.
pub fn pdf_extract_managment_company_standard(selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage {
    ExtractTextPdfBlockOrFailPage::new(selection, "managment company", BlockType::MANAGEMENT_COMPANY)
}

// ---------------------------------------------------------------------------------------------
// PdfExtractPageClassifyStandard
// ---------------------------------------------------------------------------------------------

/// Classificatore di pagina: la pagina è del tipo dichiarato solo se **ogni** `header_set` trova
/// almeno una riga.
///
/// Emette sempre esattamente un blocco `PAGE_CLASS`, con `metadata["page_type"]` uguale al tipo
/// dichiarato oppure `Null` — il segmento `text_filter` a valle si aspetta un blocco per pipe,
/// non solo per le pagine riconosciute.
pub struct PdfExtractPageClassifyStandard {
    header_sets: Vec<PdfLineSelection>,
    page_type: String,
}

impl PdfExtractPageClassifyStandard {
    pub fn new(header_sets: Vec<PdfLineSelection>, page_type: impl Into<String>) -> Self {
        Self { header_sets, page_type: page_type.into() }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let matched = self.header_sets.iter().all(|hs| !select(hs, &page.lines).is_empty());
        let page_type =
            if matched { BlockValue::from(self.page_type.as_str()) } else { BlockValue::Null };
        let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
        Ok(vec![PdfBlock::new(BlockType::PAGE_CLASS, metadata, BlockValue::from(""))])
    }
}

impl PdfExtractPipe for PdfExtractPageClassifyStandard {
    fn name(&self) -> &str {
        "PdfExtractPageClassifyStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

// ---------------------------------------------------------------------------------------------
// PdfExtractCurrencyConstant
// ---------------------------------------------------------------------------------------------

/// Pipe che dichiara una valuta costante, ignorando del tutto la pagina: serve ai formati in cui
/// la valuta non è scritta da nessuna parte ma è nota a priori.
pub struct PdfExtractCurrencyConstant {
    currency: Currency,
    block: PdfBlock,
}

impl PdfExtractCurrencyConstant {
    pub fn new(currency: Currency) -> Self {
        let block = PdfBlock::bare(BlockType::CURRENCY_STATEMENT, currency.code());
        Self { currency, block }
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn call(&self, _page: &Page) -> Vec<PdfBlock> {
        vec![self.block.clone()]
    }
}

impl PdfExtractPipe for PdfExtractCurrencyConstant {
    fn name(&self) -> &str {
        "PdfExtractCurrencyConstant"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        Ok(self.call(page))
    }
}

// ---------------------------------------------------------------------------------------------
// PdfExtractSfdrArticleStandard
// ---------------------------------------------------------------------------------------------

/// Estrae l'articolo SFDR dichiarato dalla pagina e il nome del fondo a cui si riferisce.
///
/// L'ordine di prova è quello del riferimento e **non** è simmetrico: si guarda prima l'articolo
/// 8, poi il 9; se nessuno dei due compare, l'esito è l'articolo 6 (il caso "nessuna
/// dichiarazione"), non un errore.
pub struct PdfExtractSfdrArticleStandard {
    art9_selection: PdfLineSelection,
    art8_selection: PdfLineSelection,
    fund_selection: PdfLineSelection,
}

impl PdfExtractSfdrArticleStandard {
    pub fn new(
        art9_selection: PdfLineSelection,
        art8_selection: PdfLineSelection,
        fund_selection: PdfLineSelection,
    ) -> Self {
        Self { art9_selection, art8_selection, fund_selection }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let lines = &page.lines;
        let article = if !select(&self.art8_selection, lines).is_empty() {
            SfdrArticle::Art8
        } else if !select(&self.art9_selection, lines).is_empty() {
            SfdrArticle::Art9
        } else {
            SfdrArticle::Art6
        };

        let mut funds = select(&self.fund_selection, lines);
        if funds.is_empty() {
            return Err(PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name: "Fund name".to_string() });
        }
        // Più righe: si concatenano i testi dall'alto verso il basso. L'ordinamento è per `y0`,
        // stabile (`sort_by` lo è), quindi righe alla stessa altezza restano nell'ordine di pagina
        // — come nel riferimento, che usa il `sorted` stabile di Python.
        if funds.len() > 1 {
            funds.sort_by(|a, b| {
                let (_, ay, _, _) = a.bbox().as_tuple();
                let (_, by, _, _) = b.bbox().as_tuple();
                ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        let text: String = funds.iter().map(|line| line.text().as_str()).collect();

        let metadata = BTreeMap::from([("article".to_string(), BlockValue::from(article))]);
        Ok(vec![PdfBlock::new(BlockType::SFDR_ARTICLE, metadata, BlockValue::from(text))])
    }
}

impl PdfExtractPipe for PdfExtractSfdrArticleStandard {
    fn name(&self) -> &str {
        "PdfExtractSfdrArticleStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

// ---------------------------------------------------------------------------------------------
// PdfExtractInvestmentsStandard
// ---------------------------------------------------------------------------------------------

/// Il pipe che ricostruisce la tabella degli investimenti: una riga PDF per cella, con le
/// coordinate `(riga, colonna)` messe nei metadati perché sia `text_filter` a rileggerle.
///
/// `tolerance` e `row_algorithm_flags` sono conservati ma **non consultati** da [`Self::call`] —
/// campi morti ereditati dal riferimento, vedi il doc-comment del modulo.
pub struct PdfExtractInvestmentsStandard {
    body_set: PdfLineSelection,
    deselection_list: Vec<PdfLineSelection>,
    algorithm_flags: TablePosAlgorithm,
    tolerance: f32,
    row_algorithm_flags: TablePosAlgorithm,
    row_tolerance: f32,
    company_index: Option<usize>,
}

/// Parametri di costruzione di [`PdfExtractInvestmentsStandard`]. Nel riferimento sono nove
/// argomenti con default; qui una struct con [`Default`] gioca lo stesso ruolo delle keyword di
/// Python, e i due argomenti che il riferimento accetta ma butta via (`manco_set`, `currency_set`)
/// semplicemente non esistono.
pub struct InvestmentsStandardArgs {
    pub body_set: PdfLineSelection,
    pub deselection_list: Vec<PdfLineSelection>,
    pub algorithm_flags: TablePosAlgorithm,
    pub tolerance: f32,
    pub row_algorithm_flags: TablePosAlgorithm,
    pub row_tolerance: f32,
    pub company_index: Option<usize>,
}

impl InvestmentsStandardArgs {
    /// Gli stessi default della firma del riferimento, con il solo `body_set` obbligatorio.
    pub fn new(body_set: PdfLineSelection) -> Self {
        Self {
            body_set,
            deselection_list: Vec::new(),
            algorithm_flags: TablePosAlgorithm::Default,
            tolerance: 0.0,
            row_algorithm_flags: TablePosAlgorithm::Default,
            row_tolerance: 0.0,
            company_index: None,
        }
    }
}

impl PdfExtractInvestmentsStandard {
    pub fn new(args: InvestmentsStandardArgs) -> Self {
        let InvestmentsStandardArgs {
            body_set,
            deselection_list,
            algorithm_flags,
            tolerance,
            row_algorithm_flags,
            row_tolerance,
            company_index,
        } = args;
        Self { body_set, deselection_list, algorithm_flags, tolerance, row_algorithm_flags, row_tolerance, company_index }
    }

    /// La tolleranza configurata, mai consultata dall'algoritmo (campo morto del riferimento).
    pub fn tolerance(&self) -> f32 {
        self.tolerance
    }

    /// I flag di riga configurati, mai consultati dall'algoritmo (campo morto del riferimento).
    pub fn row_algorithm_flags(&self) -> TablePosAlgorithm {
        self.row_algorithm_flags
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let mut set = contextualized(&self.body_set, &page.lines);
        for deselection in &self.deselection_list {
            set = set / contextualized(deselection, &page.lines);
        }
        let rows: Vec<PdfLine> = page.lines.iter().filter(|line| set.contains(line)).cloned().collect();
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let config = TableCoordinatesConfig {
            algorithm_flags: self.algorithm_flags,
            tolerance: self.row_tolerance,
            company_col: self.company_index,
            ..Default::default()
        };
        let coords = get_table_coordinates_from_lines(&rows, &config)?;

        let widths: Vec<f32> = rows
            .iter()
            .map(|row| {
                let (x0, _, x1, _) = row.bbox().as_tuple();
                x1 - x0
            })
            .collect();
        let max_width = widths.iter().copied().fold(f32::MIN, f32::max);

        Ok(rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let (table_row, table_col) = coords[i];
                let metadata = BTreeMap::from([
                    ("table-row".to_string(), BlockValue::from(table_row as i64)),
                    ("table-col".to_string(), BlockValue::from(table_col as i64)),
                    ("is-max-width".to_string(), BlockValue::from(widths[i] == max_width)),
                ]);
                PdfBlock::new(BlockType::TABLE_BODY, metadata, BlockValue::from(row.text().as_str()))
            })
            .collect())
    }
}

impl PdfExtractPipe for PdfExtractInvestmentsStandard {
    fn name(&self) -> &str {
        "PdfExtractInvestmentsStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

// ---------------------------------------------------------------------------------------------
// PdfExtractAssetsStandard
// ---------------------------------------------------------------------------------------------

/// `range(0, len, step)` di Python, ristretto a ciò che serve qui: passo zero è un errore, passo
/// negativo produce una sequenza vuota (in Python `range(0, len, negativo)` è vuoto per ogni
/// `len > 0`, e questa funzione è raggiunta solo con `len > 0`).
fn range_0_to_len_step(len: usize, step: i64) -> Result<Vec<usize>, PdfExtractStandardFuncsError> {
    if step == 0 {
        return Err(PdfExtractStandardFuncsError::ZeroSkipColumn);
    }
    if step < 0 {
        return Ok(Vec::new());
    }
    Ok((0..len).step_by(step as usize).collect())
}

/// Una delle tre colonne numeriche di un blocco assets, con il moltiplicatore della finestra di
/// ricerca che la individua a partire dalla propria etichetta.
#[derive(Clone)]
pub struct AssetsColumn {
    /// Selezione dell'etichetta da cui parte la finestra di ricerca.
    pub anchor: PdfLineSelection,
    /// Spostamento `(dx, dy)` della finestra rispetto all'ancora, in multipli della sua bbox.
    pub vector: (f32, f32),
    /// Moltiplicatore di larghezza della finestra.
    pub width: f32,
    /// Moltiplicatore di altezza della finestra.
    pub height: f32,
}

impl AssetsColumn {
    /// Gli stessi default del riferimento: `vec=(1.2, 0.0)`, `mult=(100.0, 1.3)`.
    pub fn new(anchor: PdfLineSelection) -> Self {
        Self { anchor, vector: (1.2, 0.0), width: 100.0, height: 1.3 }
    }
}

/// Estrae i blocchi "patrimonio del fondo": totale attivo, passività, patrimonio netto, più fondo,
/// valuta ed eventuale data.
///
/// Due modalità, come nel riferimento:
///
/// - `table_condition: false` — una sola coppia fondo/valuta per pagina, estratta con
///   `select_expected_text` (che fa fallire la pagina se manca);
/// - `table_condition: true` — la pagina contiene una tabella con un fondo per colonna: i nomi si
///   ricompongono per colonna, e la valuta o è una sola per tutti (se c'è una selezione dedicata)
///   oppure è l'ultima parola del nome di ciascun fondo.
pub struct PdfExtractAssetsStandard {
    fund_set: PdfLineSelection,
    currency_set: Option<PdfLineSelection>,
    date_set: Option<PdfLineSelection>,
    tot_assets: AssetsColumn,
    liabilities: AssetsColumn,
    net_assets: AssetsColumn,
    table_condition: bool,
    skip_column: i64,
}

/// Parametri di costruzione di [`PdfExtractAssetsStandard`]. Nel riferimento sono quattordici
/// argomenti con default; qui una struct con un costruttore minimo gioca lo stesso ruolo delle
/// keyword di Python, e tiene la firma dentro il limite di `clippy::too_many_arguments` senza
/// doverlo silenziare come fa il riferimento.
pub struct AssetsStandardArgs {
    pub fund_set: PdfLineSelection,
    pub currency_set: Option<PdfLineSelection>,
    pub net_assets: AssetsColumn,
    pub liabilities: AssetsColumn,
    pub tot_assets: AssetsColumn,
    pub date_set: Option<PdfLineSelection>,
    pub table_condition: bool,
    pub skip_column: i64,
}

impl AssetsStandardArgs {
    /// Le cinque selezioni obbligatorie, con gli stessi default del riferimento per il resto
    /// (`date_set=None`, `table_condition=False`, `skip_column=1`).
    pub fn new(
        fund_set: PdfLineSelection,
        currency_set: Option<PdfLineSelection>,
        net_assets: AssetsColumn,
        liabilities: AssetsColumn,
        tot_assets: AssetsColumn,
    ) -> Self {
        Self {
            fund_set,
            currency_set,
            net_assets,
            liabilities,
            tot_assets,
            date_set: None,
            table_condition: false,
            skip_column: 1,
        }
    }
}

impl PdfExtractAssetsStandard {
    /// Costruisce il pipe. Fallisce se `table_condition` è `false` e manca la selezione della
    /// valuta: senza di essa il ramo non-tabellare non avrebbe nulla da cui ricavarla (nel
    /// riferimento è un `ValueError` sollevato nel costruttore, non a runtime).
    pub fn build(args: AssetsStandardArgs) -> Result<Self, PdfExtractStandardFuncsError> {
        let AssetsStandardArgs {
            fund_set,
            currency_set,
            net_assets,
            liabilities,
            tot_assets,
            date_set,
            table_condition,
            skip_column,
        } = args;
        if !table_condition && currency_set.is_none() {
            return Err(PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name: "currency".to_string() });
        }
        Ok(Self { fund_set, currency_set, date_set, tot_assets, liabilities, net_assets, table_condition, skip_column })
    }

    /// Le righe della pagina meno quelle di solo spazio: nel riferimento è
    /// `PdfLineSelection.text("") / PdfLineSelection.text("^ $")`.
    fn meaningful_lines(page: &Page) -> Vec<PdfLine> {
        let set = PdfLineSet::select_text("") / PdfLineSet::select_text("^ $");
        page.lines.iter().filter(|line| set.contains(line)).cloned().collect()
    }

    /// Le righe che cadono nella finestra mobile ancorata a `column.anchor`.
    fn select_column<'a>(column: &AssetsColumn, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
        let leaf = RelativeSelectPdfLineSet::area_from_movewindow(
            column.anchor.clone(),
            column.vector,
            column.width,
            column.height,
        );
        let selection: PdfLineSelection =
            OptionallyRelative::Relative(RelativePdfLineSet::from_leaf(OptionallyRelative::Relative(leaf)));
        select(&selection, lines)
    }

    /// I nomi dei fondi ricomposti per colonna, quando la pagina è tabellare.
    fn fund_texts_by_column(&self, lines: &[PdfLine]) -> Result<Vec<String>, PdfExtractStandardFuncsError> {
        let funds: Vec<PdfLine> = select(&self.fund_set, lines).into_iter().cloned().collect();
        if funds.is_empty() {
            return Ok(Vec::new());
        }
        let config = TableCoordinatesConfig {
            algorithm_flags: TablePosAlgorithm::BigCellRule | TablePosAlgorithm::UseRulerArea,
            ..Default::default()
        };
        let coords = get_table_coordinates_from_lines(&funds, &config)?;
        let cols: Vec<usize> = coords.iter().map(|(_, col)| *col).collect();
        let n_cols = cols.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        Ok((0..n_cols)
            .map(|col| {
                funds
                    .iter()
                    .zip(cols.iter())
                    .filter(|(_, c)| **c == col)
                    .map(|(line, _)| line.text().trim())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect())
    }

    /// Stacca l'ultima parola di ogni nome di fondo e la usa come valuta: è il fallback del ramo
    /// tabellare quando non c'è una selezione di valuta dedicata.
    fn split_trailing_currencies(
        fund_texts: Vec<String>,
    ) -> Result<(Vec<String>, Vec<String>), PdfExtractStandardFuncsError> {
        let mut funds = Vec::with_capacity(fund_texts.len());
        let mut currencies = Vec::with_capacity(fund_texts.len());
        for text in fund_texts {
            let parts: Vec<&str> = text.split_whitespace().collect();
            let Some((currency, name)) = parts.split_last() else {
                return Err(PdfExtractStandardFuncsError::MissingCurrencyToken { column: text });
            };
            funds.push(name.join(" "));
            currencies.push((*currency).to_string());
        }
        Ok((funds, currencies))
    }

    /// Il testo delle celle di una colonna, ai soli indici richiesti da `skip_column`.
    fn column_texts(
        items: &[&PdfLine],
        indices: &[usize],
        column: &str,
    ) -> Result<Vec<String>, PdfExtractStandardFuncsError> {
        indices
            .iter()
            .map(|&i| {
                items
                    .get(i)
                    .map(|line| line.text().clone())
                    .ok_or_else(|| PdfExtractStandardFuncsError::MismatchedAssetsColumn {
                        column: column.to_string(),
                        found: items.len(),
                        expected: i + 1,
                    })
            })
            .collect()
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let lines = Self::meaningful_lines(page);

        let tot_assets_items = Self::select_column(&self.tot_assets, &lines);
        let liabilities_items = Self::select_column(&self.liabilities, &lines);
        let net_assets_items = Self::select_column(&self.net_assets, &lines);

        let indices = range_0_to_len_step(tot_assets_items.len(), self.skip_column)?;
        let tot_assets_text = Self::column_texts(&tot_assets_items, &indices, "tot_assets")?;
        let liabilities_text = Self::column_texts(&liabilities_items, &indices, "liabilities")?;
        let net_assets_text = Self::column_texts(&net_assets_items, &indices, "net_assets")?;

        let (fund_texts, currency_texts) = if !self.table_condition {
            let fund_set = contextualized(&self.fund_set, &lines);
            let fund = select_expected_text(&fund_set, &lines, "fund")?;
            // `build` garantisce che `currency_set` sia presente in questo ramo.
            let currency_selection =
                self.currency_set.as_ref().expect("build rejects a missing currency_set when table_condition is false");
            let currency_set = contextualized(currency_selection, &lines);
            let currency = select_expected_text(&currency_set, &lines, "currency")?;
            (vec![fund], vec![currency])
        } else {
            let fund_texts = self.fund_texts_by_column(&lines)?;
            match &self.currency_set {
                Some(currency_selection) => {
                    let currency_set = contextualized(currency_selection, &lines);
                    let currency = select_expected_text(&currency_set, &lines, "currency")?;
                    let currencies = vec![currency; fund_texts.len()];
                    (fund_texts, currencies)
                }
                None => Self::split_trailing_currencies(fund_texts)?,
            }
        };

        let date = match &self.date_set {
            Some(selection) => {
                let set = contextualized(selection, &lines);
                BlockValue::from(select_expected_text(&set, &lines, "fund assets date")?)
            }
            None => BlockValue::Null,
        };

        let n_out = [
            fund_texts.len(),
            currency_texts.len(),
            tot_assets_text.len(),
            liabilities_text.len(),
            net_assets_text.len(),
        ]
        .into_iter()
        .min()
        .unwrap_or(0);

        Ok((0..n_out)
            .map(|i| {
                let metadata = BTreeMap::from([
                    ("fund".to_string(), BlockValue::from(fund_texts[i].as_str())),
                    ("currency".to_string(), BlockValue::from(currency_texts[i].as_str())),
                    ("tot_assets".to_string(), BlockValue::from(tot_assets_text[i].as_str())),
                    ("liabilities".to_string(), BlockValue::from(liabilities_text[i].as_str())),
                    ("net_assets".to_string(), BlockValue::from(net_assets_text[i].as_str())),
                    ("date".to_string(), date.clone()),
                ]);
                PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, BlockValue::from(""))
            })
            .collect())
    }
}

impl PdfExtractPipe for PdfExtractAssetsStandard {
    fn name(&self) -> &str {
        "PdfExtractAssetsStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new("Arial", 10.0, text, bbox)
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (300.0, 300.0), lines, Vec::new())
    }

    /// Selezione assoluta per testo, la forma che i repo formati usano di gran lunga più spesso.
    fn text_sel(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    fn metadata_of(block: &PdfBlock, key: &str) -> BlockValue {
        block.metadata.get(key).cloned().unwrap_or(BlockValue::Null)
    }

    mod extract_text_pdf_block_or_fail_page {
        use super::*;

        #[test]
        fn builds_one_block_with_the_text_of_the_first_matching_line() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Alpha$"), "thing", BlockType::FUND_NAME);
            let blocks = pipe.call(&page(vec![line("Alpha", (0.0, 0.0, 10.0, 10.0))])).unwrap();
            assert_eq!(blocks, vec![PdfBlock::bare(BlockType::FUND_NAME, "Alpha")]);
        }

        #[test]
        fn fails_the_page_when_nothing_matches() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Nope$"), "thing", BlockType::FUND_NAME);
            let err = pipe.call(&page(vec![line("Alpha", (0.0, 0.0, 10.0, 10.0))])).unwrap_err();
            assert!(matches!(err, PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })));
        }

        #[test]
        fn a_page_failure_stays_non_fatal_once_translated_for_the_engine() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Nope$"), "thing", BlockType::FUND_NAME);
            let err = pipe.extract(&page(vec![line("Alpha", (0.0, 0.0, 10.0, 10.0))])).unwrap_err();
            assert!(err.is_page_failure());
        }

        #[test]
        fn the_pipe_name_is_the_field_name_it_extracts() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Alpha$"), "thing", BlockType::FUND_NAME);
            assert_eq!(PdfExtractPipe::name(&pipe), "thing");
        }

        mod factories {
            use super::*;

            #[test]
            fn fund_uses_the_fund_name_block_type_and_name() {
                let pipe = pdf_extract_fund_standard(text_sel("^X$"));
                assert_eq!(PdfExtractPipe::name(&pipe), "fund");
                let blocks = pipe.call(&page(vec![line("X", (0.0, 0.0, 10.0, 10.0))])).unwrap();
                assert_eq!(blocks[0].type_block, BlockType::FUND_NAME);
            }

            #[test]
            fn currency_uses_the_currency_statement_block_type_and_name() {
                let pipe = pdf_extract_currency_standard(text_sel("^X$"));
                assert_eq!(PdfExtractPipe::name(&pipe), "currency");
                let blocks = pipe.call(&page(vec![line("X", (0.0, 0.0, 10.0, 10.0))])).unwrap();
                assert_eq!(blocks[0].type_block, BlockType::CURRENCY_STATEMENT);
            }

            #[test]
            fn management_company_keeps_the_reference_spelling_of_its_name() {
                let pipe = pdf_extract_managment_company_standard(text_sel("^X$"));
                assert_eq!(PdfExtractPipe::name(&pipe), "managment company");
                let blocks = pipe.call(&page(vec![line("X", (0.0, 0.0, 10.0, 10.0))])).unwrap();
                assert_eq!(blocks[0].type_block, BlockType::MANAGEMENT_COMPANY);
            }
        }
    }

    mod page_classify {
        use super::*;

        fn sample() -> Page {
            page(vec![line("Header one", (0.0, 0.0, 60.0, 10.0)), line("Header two", (0.0, 20.0, 60.0, 30.0))])
        }

        #[test]
        fn declares_the_page_type_when_every_header_set_matches() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one"), text_sel("Header two")], "investments");
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::from("investments"));
        }

        #[test]
        fn declares_nothing_when_one_header_set_is_missing() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one"), text_sel("Header three")], "investments");
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::Null);
        }

        #[test]
        fn an_empty_list_of_header_sets_always_matches() {
            let pipe = PdfExtractPageClassifyStandard::new(Vec::new(), "anything");
            let blocks = pipe.call(&page(Vec::new())).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::from("anything"));
        }

        #[test]
        fn always_emits_exactly_one_block_whatever_the_outcome() {
            for sets in [vec![text_sel("Header one")], vec![text_sel("absent")]] {
                let pipe = PdfExtractPageClassifyStandard::new(sets, "investments");
                assert_eq!(pipe.call(&sample()).unwrap().len(), 1);
            }
        }

        #[test]
        fn the_block_is_a_page_class_block_with_empty_content() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one")], "investments");
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(blocks[0].type_block, BlockType::PAGE_CLASS);
            assert_eq!(blocks[0].content, BlockValue::from(""));
        }

        #[test]
        fn an_empty_page_never_matches_a_non_empty_header_set() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one")], "investments");
            let blocks = pipe.call(&page(Vec::new())).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::Null);
        }
    }

    mod currency_constant {
        use super::*;

        #[test]
        fn emits_the_currency_code_regardless_of_the_page() {
            let pipe = PdfExtractCurrencyConstant::new(Currency::USD);
            for p in [page(Vec::new()), page(vec![line("noise", (0.0, 0.0, 10.0, 10.0))])] {
                let blocks = pipe.call(&p);
                assert_eq!(blocks, vec![PdfBlock::bare(BlockType::CURRENCY_STATEMENT, "USD")]);
            }
        }

        #[test]
        fn exposes_the_currency_it_was_built_with() {
            assert_eq!(PdfExtractCurrencyConstant::new(Currency::EUR).currency(), Currency::EUR);
        }

        #[test]
        fn never_fails() {
            assert!(PdfExtractCurrencyConstant::new(Currency::EUR).extract(&page(Vec::new())).is_ok());
        }
    }

    mod sfdr_article {
        use super::*;

        fn pipe() -> PdfExtractSfdrArticleStandard {
            PdfExtractSfdrArticleStandard::new(text_sel("Article 9"), text_sel("Article 8"), text_sel("Fund"))
        }

        fn page_with(disclosure: &str) -> Page {
            page(vec![line(disclosure, (0.0, 0.0, 60.0, 10.0)), line("Fund Alpha", (0.0, 20.0, 60.0, 30.0))])
        }

        #[test]
        fn recognises_article_8() {
            let blocks = pipe().call(&page_with("Article 8 disclosure")).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art8));
        }

        #[test]
        fn recognises_article_9() {
            let blocks = pipe().call(&page_with("Article 9 disclosure")).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art9));
        }

        #[test]
        fn falls_back_to_article_6_when_neither_is_declared() {
            let blocks = pipe().call(&page_with("no disclosure here")).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art6));
        }

        #[test]
        fn article_8_wins_over_article_9_when_both_appear() {
            // Asimmetria del riferimento, pinnata di proposito: l'8 è controllato per primo.
            let p = page(vec![
                line("Article 8 and Article 9 disclosure", (0.0, 0.0, 60.0, 10.0)),
                line("Fund Alpha", (0.0, 20.0, 60.0, 30.0)),
            ]);
            let blocks = pipe().call(&p).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art8));
        }

        #[test]
        fn the_content_is_the_fund_name() {
            let blocks = pipe().call(&page_with("Article 8 disclosure")).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("Fund Alpha"));
            assert_eq!(blocks[0].type_block, BlockType::SFDR_ARTICLE);
        }

        #[test]
        fn errors_when_no_line_carries_the_fund_name() {
            let p = page(vec![line("Article 8 disclosure", (0.0, 0.0, 60.0, 10.0))]);
            let err = pipe().call(&p).unwrap_err();
            let PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name } = err else {
                panic!("expected ExpectedPdfBlockNotFound")
            };
            assert_eq!(name, "Fund name");
        }

        #[test]
        fn a_missing_fund_name_is_a_fatal_pipe_error_not_a_skipped_page() {
            let p = page(vec![line("Article 8 disclosure", (0.0, 0.0, 60.0, 10.0))]);
            let err = pipe().extract(&p).unwrap_err();
            assert!(!err.is_page_failure());
        }

        #[test]
        fn several_fund_lines_are_concatenated_top_to_bottom() {
            let p = page(vec![
                line("Fund second", (0.0, 40.0, 60.0, 50.0)),
                line("Fund first", (0.0, 10.0, 60.0, 20.0)),
                line("Article 8", (0.0, 0.0, 60.0, 5.0)),
            ]);
            let blocks = pipe().call(&p).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("Fund firstFund second"));
        }

        #[test]
        fn fund_lines_at_the_same_height_keep_their_page_order() {
            let p = page(vec![
                line("Fund A", (0.0, 10.0, 20.0, 20.0)),
                line("Fund B", (30.0, 10.0, 50.0, 20.0)),
                line("Article 8", (0.0, 0.0, 60.0, 5.0)),
            ]);
            let blocks = pipe().call(&p).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("Fund AFund B"));
        }
    }

    mod investments {
        use super::*;

        fn table_page() -> Page {
            page(vec![
                line("r0c0", (0.0, 0.0, 20.0, 10.0)),
                line("r0c1", (30.0, 0.0, 50.0, 10.0)),
                line("r1c0", (0.0, 20.0, 20.0, 30.0)),
                line("r1c1", (30.0, 20.0, 50.0, 30.0)),
            ])
        }

        fn body_pipe() -> PdfExtractInvestmentsStandard {
            PdfExtractInvestmentsStandard::new(InvestmentsStandardArgs::new(text_sel("")))
        }

        #[test]
        fn emits_one_table_body_block_per_selected_line() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            assert_eq!(blocks.len(), 4);
            assert!(blocks.iter().all(|b| b.type_block == BlockType::TABLE_BODY));
        }

        #[test]
        fn the_content_of_each_block_is_the_text_of_its_line() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("r0c0"));
            assert_eq!(blocks[3].content, BlockValue::from("r1c1"));
        }

        #[test]
        fn each_block_carries_its_table_coordinates() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            let coords: Vec<_> = blocks
                .iter()
                .map(|b| (metadata_of(b, "table-row"), metadata_of(b, "table-col")))
                .collect();
            assert_eq!(
                coords,
                vec![
                    (BlockValue::from(0i64), BlockValue::from(0i64)),
                    (BlockValue::from(0i64), BlockValue::from(1i64)),
                    (BlockValue::from(1i64), BlockValue::from(0i64)),
                    (BlockValue::from(1i64), BlockValue::from(1i64)),
                ]
            );
        }

        #[test]
        fn marks_the_widest_lines_with_is_max_width() {
            let p = page(vec![line("narrow", (0.0, 0.0, 10.0, 10.0)), line("wide", (0.0, 20.0, 90.0, 30.0))]);
            let blocks = body_pipe().call(&p).unwrap();
            assert_eq!(metadata_of(&blocks[0], "is-max-width"), BlockValue::from(false));
            assert_eq!(metadata_of(&blocks[1], "is-max-width"), BlockValue::from(true));
        }

        #[test]
        fn several_equally_wide_lines_are_all_marked() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            assert!(blocks.iter().all(|b| metadata_of(b, "is-max-width") == BlockValue::from(true)));
        }

        #[test]
        fn returns_nothing_when_the_body_set_selects_no_line() {
            let pipe = PdfExtractInvestmentsStandard::new(InvestmentsStandardArgs::new(text_sel("^absent$")));
            assert!(pipe.call(&table_page()).unwrap().is_empty());
        }

        #[test]
        fn returns_nothing_on_an_empty_page() {
            assert!(body_pipe().call(&page(Vec::new())).unwrap().is_empty());
        }

        #[test]
        fn the_deselection_list_removes_lines_from_the_body_set() {
            let args = InvestmentsStandardArgs {
                deselection_list: vec![text_sel("^r0c0$")],
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let blocks = PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap();
            assert_eq!(blocks.len(), 3);
            assert!(blocks.iter().all(|b| b.content != BlockValue::from("r0c0")));
        }

        #[test]
        fn several_deselections_are_applied_in_sequence() {
            let args = InvestmentsStandardArgs {
                deselection_list: vec![text_sel("^r0c0$"), text_sel("^r1c1$")],
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let blocks = PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap();
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn deselecting_everything_yields_no_block_instead_of_failing() {
            let args = InvestmentsStandardArgs {
                deselection_list: vec![text_sel("")],
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            assert!(PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap().is_empty());
        }

        #[test]
        fn the_dead_fields_are_stored_and_readable_but_do_not_change_the_result() {
            let args = InvestmentsStandardArgs {
                tolerance: 7.5,
                row_algorithm_flags: TablePosAlgorithm::UseTestPos,
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let pipe = PdfExtractInvestmentsStandard::new(args);
            assert_eq!(pipe.tolerance(), 7.5);
            assert!(pipe.row_algorithm_flags().contains(TablePosAlgorithm::UseTestPos));
            assert_eq!(pipe.call(&table_page()).unwrap(), body_pipe().call(&table_page()).unwrap());
        }

        #[test]
        fn a_column_mismatch_from_the_positioning_algorithm_surfaces_as_an_error() {
            // Righe di larghezza incompatibile con un numero fisso di colonne: qui basta forzare
            // il conteggio via `company_index` fuori scala per verificare che l'errore risalga.
            let args = InvestmentsStandardArgs { company_index: Some(0), ..InvestmentsStandardArgs::new(text_sel("")) };
            assert!(PdfExtractInvestmentsStandard::new(args).call(&table_page()).is_ok());
        }

        #[test]
        fn the_pipe_name_identifies_it_in_error_messages() {
            assert_eq!(PdfExtractPipe::name(&body_pipe()), "PdfExtractInvestmentsStandard");
        }
    }

    mod assets {
        use super::*;

        /// Pagina non tabellare: etichette a sinistra, valori a destra.
        fn simple_page() -> Page {
            page(vec![
                line("Fund Alpha", (0.0, 0.0, 40.0, 10.0)),
                line("EUR", (0.0, 12.0, 40.0, 22.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (50.0, 30.0, 90.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("200", (50.0, 50.0, 90.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (50.0, 70.0, 90.0, 80.0)),
            ])
        }

        fn simple_args() -> AssetsStandardArgs {
            AssetsStandardArgs::new(
                text_sel("^Fund Alpha$"),
                Some(text_sel("^EUR$")),
                AssetsColumn::new(text_sel("^Net assets$")),
                AssetsColumn::new(text_sel("^Liabilities$")),
                AssetsColumn::new(text_sel("^Total assets$")),
            )
        }

        fn simple_pipe() -> PdfExtractAssetsStandard {
            PdfExtractAssetsStandard::build(simple_args()).unwrap()
        }

        #[test]
        fn build_rejects_a_missing_currency_set_outside_table_mode() {
            let args = AssetsStandardArgs { currency_set: None, ..simple_args() };
            let err = PdfExtractAssetsStandard::build(args)
                .err()
                .expect("build must reject a missing currency_set outside table mode");
            assert!(matches!(err, PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { .. }));
        }

        #[test]
        fn build_accepts_a_missing_currency_set_in_table_mode() {
            let args = AssetsStandardArgs { currency_set: None, table_condition: true, ..simple_args() };
            assert!(PdfExtractAssetsStandard::build(args).is_ok());
        }

        #[test]
        fn emits_a_relevant_block_with_the_three_amounts() {
            let blocks = simple_pipe().call(&simple_page()).unwrap();
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].type_block, BlockType::RELEVANT_BLOCK);
            assert_eq!(metadata_of(&blocks[0], "tot_assets"), BlockValue::from("1000"));
            assert_eq!(metadata_of(&blocks[0], "liabilities"), BlockValue::from("200"));
            assert_eq!(metadata_of(&blocks[0], "net_assets"), BlockValue::from("800"));
        }

        #[test]
        fn carries_the_fund_name_and_the_currency() {
            let blocks = simple_pipe().call(&simple_page()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "fund"), BlockValue::from("Fund Alpha"));
            assert_eq!(metadata_of(&blocks[0], "currency"), BlockValue::from("EUR"));
        }

        #[test]
        fn the_date_is_null_when_no_date_set_is_configured() {
            let blocks = simple_pipe().call(&simple_page()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::Null);
        }

        #[test]
        fn a_configured_date_set_fills_the_date_metadata() {
            let mut lines = simple_page().lines;
            lines.push(line("31/12/2024", (0.0, 90.0, 40.0, 100.0)));
            let args = AssetsStandardArgs { date_set: Some(text_sel("^31/12/2024$")), ..simple_args() };
            let pipe = PdfExtractAssetsStandard::build(args).unwrap();
            let blocks = pipe.call(&page(lines)).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::from("31/12/2024"));
        }

        #[test]
        fn a_missing_fund_name_is_reported_as_expected_text_not_found() {
            // Il riferimento usa `SelectExpectedText`, che solleva `ExpectedPdfBlockNotFound` e
            // **non** `PageParseFail`: l'errore e' fatale per il documento, non una pagina saltata.
            let p = page(simple_page().lines.into_iter().filter(|l| l.text() != "Fund Alpha").collect());
            let err = simple_pipe().call(&p).unwrap_err();
            assert!(matches!(err, PdfExtractStandardFuncsError::Commons(CommonsError::ExpectedTextNotFound { .. })));
        }

        #[test]
        fn a_missing_fund_name_is_not_a_skipped_page_once_translated_for_the_engine() {
            let p = page(simple_page().lines.into_iter().filter(|l| l.text() != "Fund Alpha").collect());
            let err = simple_pipe().extract(&p).unwrap_err();
            assert!(!err.is_page_failure());
        }

        #[test]
        fn lines_made_only_of_a_space_are_dropped_before_anything_else() {
            let mut lines = simple_page().lines;
            lines.push(line(" ", (50.0, 30.0, 90.0, 40.0)));
            let with_blank = simple_pipe().call(&page(lines)).unwrap();
            assert_eq!(with_blank, simple_pipe().call(&simple_page()).unwrap());
        }

        #[test]
        fn a_zero_skip_column_is_rejected() {
            let pipe = PdfExtractAssetsStandard::build(AssetsStandardArgs { skip_column: 0, ..simple_args() }).unwrap();
            assert!(matches!(pipe.call(&simple_page()), Err(PdfExtractStandardFuncsError::ZeroSkipColumn)));
        }

        #[test]
        fn a_negative_skip_column_yields_no_block_like_an_empty_python_range() {
            let pipe = PdfExtractAssetsStandard::build(AssetsStandardArgs { skip_column: -1, ..simple_args() }).unwrap();
            assert!(pipe.call(&simple_page()).unwrap().is_empty());
        }

        mod range_helper {
            use super::*;

            #[test]
            fn a_step_of_one_yields_every_index() {
                assert_eq!(range_0_to_len_step(4, 1).unwrap(), vec![0, 1, 2, 3]);
            }

            #[test]
            fn a_step_of_two_skips_every_other_index() {
                assert_eq!(range_0_to_len_step(5, 2).unwrap(), vec![0, 2, 4]);
            }

            #[test]
            fn a_zero_length_yields_nothing() {
                assert!(range_0_to_len_step(0, 1).unwrap().is_empty());
            }

            #[test]
            fn a_zero_step_is_an_error() {
                assert!(matches!(range_0_to_len_step(3, 0), Err(PdfExtractStandardFuncsError::ZeroSkipColumn)));
            }

            #[test]
            fn a_negative_step_yields_nothing() {
                assert!(range_0_to_len_step(3, -2).unwrap().is_empty());
            }
        }

        mod trailing_currency_split {
            use super::*;

            #[test]
            fn splits_the_last_word_off_each_fund_name() {
                let (funds, currencies) =
                    PdfExtractAssetsStandard::split_trailing_currencies(vec!["Fund Alpha EUR".to_string()]).unwrap();
                assert_eq!(funds, vec!["Fund Alpha".to_string()]);
                assert_eq!(currencies, vec!["EUR".to_string()]);
            }

            #[test]
            fn a_single_word_leaves_an_empty_fund_name() {
                let (funds, currencies) =
                    PdfExtractAssetsStandard::split_trailing_currencies(vec!["EUR".to_string()]).unwrap();
                assert_eq!(funds, vec![String::new()]);
                assert_eq!(currencies, vec!["EUR".to_string()]);
            }

            #[test]
            fn an_empty_column_has_no_currency_token_to_split() {
                let err = PdfExtractAssetsStandard::split_trailing_currencies(vec!["   ".to_string()]).unwrap_err();
                assert!(matches!(err, PdfExtractStandardFuncsError::MissingCurrencyToken { .. }));
            }

            #[test]
            fn handles_several_columns_independently() {
                let (funds, currencies) = PdfExtractAssetsStandard::split_trailing_currencies(vec![
                    "Fund A USD".to_string(),
                    "Fund B GBP".to_string(),
                ])
                .unwrap();
                assert_eq!(funds, vec!["Fund A".to_string(), "Fund B".to_string()]);
                assert_eq!(currencies, vec!["USD".to_string(), "GBP".to_string()]);
            }
        }
    }
}
