//! Pipe `text_filter` standard — porting di
//! `freeports_core/src/formats_utils/text_filter/standard_funcs.rs`.
//!
//! **Scope.** M4 ha portato la parte che non dipendeva dal motore
//! (`TextFilterPageClassifyStandard`, `extract_currency_from_text`); M5 ha aggiunto
//! `TextFilterInvestmentsStandard` — con `PdfBlocksTable` inlined, come nel riferimento — che
//! dal `filter_data` legge **solo** le `CompanyMatchInfos` ed è quindi diventato costruibile non
//! appena `FilterData` è esistito. Restano fuori `TextFilterSfdrArticleStandard`,
//! `TextFilterManagmentCompanyStandard` e `TextFilterAssetsStandard`, che dal `filter_data`
//! estraggono `Fund`/`Equity`/`Bond`: dipendono da `output::classes` (M8), non dal motore.
//!
//! Da M5 i pipe di questo modulo implementano il trait
//! [`TextFilterPipe`](crate::core::pipeline::TextFilterPipe): il metodo `call` inerente resta
//! come API diretta e tipizzata sui suoi errori, il trait è la forma che il motore usa e che
//! traduce quegli errori in [`PipeError`].
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct TextFilterPageClassifyStandard;
//! impl TextFilterPageClassifyStandard {
//!     pub fn call(&self, pdf_blks: &[PdfBlock]) -> Result<Vec<TextBlock>, StandardFuncsError>;
//! }
//!
//! pub fn extract_currency_from_text(text: &str) -> Result<Currency, StandardFuncsError>;
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum StandardFuncsError { /* un enum locale, stesso trattamento provvisorio di
//!     CommonsError (M3, pdf_extract::commons) — verra' assorbito da PipeError in M5 */ }
//! ```
//!
//! `TextFilterPageClassifyStandard::call`: itera `pdf_blks` **nell'ordine dato**; ogni blocco
//! deve gia' avere in `metadata` la chiave `"page_type"` (impostata a monte da un pipe
//! `pdf_extract`, eventualmente a `BlockValue::Null` se quel blocco non e' classificato) — legge
//! quel campo con `metadata_or_fail("page_type")`. Se piu' di un blocco porta un `page_type`
//! diverso da `Null`, e' un errore (anche se i due valori non-null fossero uguali fra loro conta
//! comunque come "gia' trovato un valore" nel riferimento: qui pero' i test si limitano al caso
//! con valori diversi, l'unico specificato dal piano). Il risultato e' un singolo `TextBlock` di
//! tipo `BlockType::PAGE_CLASS`, costruito con `TextBlock::new` dall'**ultimo** `PdfBlock` della
//! lista (non il primo — facile da invertire per errore nel porting), con
//! `metadata = {"page_type": <valore trovato, o Null se nessuno}`. `pdf_blks` vuoto e' un errore.
//!
//! `extract_currency_from_text`: due passate, la prima vince appena trova qualcosa (bugfix gia'
//! documentato nel riferimento — un vecchio flag `found` che doveva interrompere la scansione di
//! fallback dopo il primo match non veniva mai impostato, quindi l'ultima valuta dichiarata
//! nell'enum vinceva sempre invece della prima incontrata nel testo; qui la prima passata che
//! trova qualcosa restituisce subito, senza continuare):
//! 1. Cerca ogni sotto-stringa di 3 lettere maiuscole delimitata da confini di parola (`\b`) nel
//!    testo **cosi' come scritto** (non maiuscolizzato), **nell'ordine in cui appaiono**; per la
//!    prima che e' un codice/nome valido (`Currency::from_name`), restituisce quella valuta.
//! 2. Se nessuna delle candidate sopra e' valida (incluso il caso "nessuna candidata trovata"),
//!    prova, sul testo maiuscolizzato, il codice ISO di ogni `Currency::variants()` **in ordine
//!    di dichiarazione**, delimitato da `\b`; poi prova l'alias `"EURO"` allo stesso modo. La
//!    prima che matcha vince.
//! 3. Se niente matcha in nessuna delle due passate, e' un errore.
//!
//! Nessuna delle due passate individua mai una sotto-stringa che non sia delimitata da un confine
//! di parola vero e proprio: una tripletta di lettere maiuscole incollata ad altre lettere o
//! cifre adiacenti (es. `"EURUSD"`, `"100EUR"`) non conta.
//!
//! ---
//!
//! **M8 (`agent-memory/M8-implementation-plan.md` §2/§3, passo 9): le tre funzioni restanti.**
//!
//! ```text
//! pub struct TextFilterSfdrArticleStandard { /* prefissi letterali + regex, demand_investment_funds_match */ }
//! impl TextFilterSfdrArticleStandard {
//!     pub fn new(prefix_strings: Vec<String>, prefix_patterns: Vec<String>, demand_investment_funds_match: bool)
//!         -> Result<Self, StandardFuncsError>;
//!     pub fn call(&self, pdf_blks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, StandardFuncsError>;
//! }
//!
//! pub struct TextFilterManagmentCompanyStandard;
//! impl TextFilterManagmentCompanyStandard {
//!     pub fn call(&self, pdf_blks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, StandardFuncsError>;
//! }
//!
//! pub struct TextFilterAssetsStandard { /* date_regex: Option<Regex>, remove_from_fund_regexes: Vec<Regex> */ }
//! impl TextFilterAssetsStandard {
//!     pub fn new(date_regex: Option<&str>, remove_from_fund_regexes: Vec<String>) -> Result<Self, StandardFuncsError>;
//!     pub fn call(&self, pdf_blks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, StandardFuncsError>;
//! }
//! ```
//!
//! **`TextFilterSfdrArticleStandard::call`**: prende il **primo** blocco di `pdf_blks` (lista
//! vuota -> [`StandardFuncsError::NoPdfBlocks`], già esistente — stesso errore di
//! `TextFilterPageClassifyStandard`, non un errore "non fatale" come `ExpectedTextBlockNotFound`,
//! perché nel riferimento nessuno lo cattura), legge `content` (nome fondo), toglie i prefissi
//! letterali (`str::replace`) e poi quelli a pattern (`Regex::replace_all` con `""`), in
//! quest'ordine. Costruisce un `MatchFund` dal nome ripulito; verifica appartenenza all'insieme
//! dei fondi-investimento (`Equity`/`Bond` **risolti** — `.data.fund.resolved()` — visti in
//! `data.previous()`) **solo se** `demand_investment_funds_match` è vero. Se il match non serve o
//! è soddisfatto: un blocco `SFDR_ARTICLE` con `content` = nome ripulito, `metadata` = quella del
//! primo blocco pdf (porta già `"article"`, scritta da `PdfExtractSfdrArticleStandard`, M7).
//! Altrimenti: lista vuota, non un errore.
//!
//! **`TextFilterManagmentCompanyStandard::call`**: costruisce l'insieme dei `MatchFund` dai
//! `Fund` **risolti** (`.name()`) visti in `data.previous()`; cerca il **primo** blocco
//! `MANAGEMENT_COMPANY` fra `pdf_blks` (nessuno trovato ->
//! [`StandardFuncsError::ExpectedTextBlockNotFound`], variante già esistente); chiama
//! `standard_management_company_txt_blk` (M4) — non lo reimplementa.
//!
//! **`TextFilterAssetsStandard::call`**: itera **tutti** i `pdf_blks` (nessun filtro per
//! `type_block`: si presume che il segmento gli passi solo `RELEVANT_BLOCK` prodotti da
//! `PdfExtractAssetsStandard`), legge `metadata["fund"]` (obbligatorio), applica
//! `remove_from_fund_regexes` (0+ pattern, sostituzione con `""`, applicati **prima** del
//! confronto), verifica appartenenza all'insieme dei `Fund` risolti di `data.previous()` — un
//! fondo non presente **non produce nulla per quel blocco** (non un errore, il ciclo continua con
//! gli altri blocchi). Se presente: applica opzionalmente `date_regex` su `metadata["date"]` (un
//! solo gruppo catturante — validato **a costruzione**, non a chiamata: un pattern con zero o più
//! di un gruppo è un [`StandardFuncsError::InvalidPattern`] al momento di `new`, non un panic su
//! `.at(1)` a runtime); un valore che non matcha affatto il pattern configurato è invece
//! [`StandardFuncsError::DateRegexMismatch`] a chiamata. Converte `metadata["currency"]` con
//! [`extract_currency_from_text`] (già in questo file) e la riscrive come `BlockValue::Currency`
//! — coerente con la decisione di `DeserializerAssetsStandard` (M8,
//! `formats_utils::deserialize::standard_funcs`) di accettare una valuta già tipizzata come
//! percorso primario. Il blocco risultante è `TextBlock::from_content(RELEVANT_BLOCK, metadata,
//! "")`, come nel riferimento.
//!
//! **Due varianti nuove di [`StandardFuncsError`]**, che l'implementer deve aggiungere (il
//! test-writer non tocca l'enum esistente): `InvalidPattern { pattern: String, message: String }`
//! (pattern regex non valido, o con un numero di gruppi catturanti sbagliato, a costruzione) e
//! `DateRegexMismatch { text: String }` (il pattern è valido ma non matcha il valore a chiamata).

use once_cell::sync::Lazy;
use onig::Regex;
use std::collections::{BTreeMap, BTreeSet};

use crate::commons::consts::Currency;
use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;
use crate::core::page::PageError;
use crate::core::pipeline::{Extracted, FilterData, PipeError, TextFilterPipe};
use crate::formats_utils::text_filter::matcher::{CompanyMatchInfos, match_company};
use crate::formats_utils::text_filter::standard_txt_blk_builders::{
    standard_fund_txt_blk, standard_management_company_txt_blk,
};
use crate::output::classes::fund::Fund;

#[derive(Debug, thiserror::Error)]
pub enum StandardFuncsError {
    #[error("expected at least one pdf block to classify")]
    NoPdfBlocks,
    #[error("page classified both as {first:?} and as {second:?}")]
    ConflictingPageClass { first: String, second: String },
    #[error(transparent)]
    Value(#[from] crate::core::classes::value::BlockValueError),
    #[error("no currency found in text")]
    NoCurrencyFound,
    /// Il testo su cui il pipe si aspettava di trovare un blocco non c'è. **Non è fatale**:
    /// `TextFilterInvestmentsStandard::run_loop` lo assorbe e passa alla riga successiva, come il
    /// riferimento fa catturando `ExpectedTextBlockNotFound`.
    #[error("matching text block not found")]
    ExpectedTextBlockNotFound,
    /// La pagina non è interpretabile: diventa un fallimento di pagina, che l'algoritmo assorbe
    /// saltando la pagina intera.
    #[error("{message}")]
    PageParseFail { message: String },
    #[error("two subfunds in the same page")]
    TwoFundsInSamePage,
    #[error("two currencies in the same page")]
    TwoCurrenciesInSamePage,
    #[error("all positions should be different")]
    PositionsMustDiffer,
    #[error("company matching failed: {message}")]
    Match { message: String },
    #[error("inconsistent investments table: {message}")]
    InconsistentTable { message: String },
    /// Un pattern regex non e' valido, o non ha il numero di gruppi catturanti richiesto —
    /// verificato a **costruzione**, non a chiamata.
    #[error("invalid pattern '{pattern}': {message}")]
    InvalidPattern { pattern: String, message: String },
    /// Il pattern e' valido ma non matcha il valore dato a chiamata.
    #[error("value '{text}' does not match the configured date pattern")]
    DateRegexMismatch { text: String },
}

impl StandardFuncsError {
    /// Traduzione nell'errore del motore. Il nome del pipe non è ricavabile dall'errore, quindi lo
    /// passa il chiamante — stessa forma di [`PipeError::from_commons`].
    ///
    /// Solo [`StandardFuncsError::PageParseFail`] diventa un fallimento **non fatale** di pagina:
    /// tutto il resto interrompe l'elaborazione, come nel riferimento, dove solo `PageParseFail`
    /// è catturato dal ciclo dello schedule.
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            StandardFuncsError::Value(source) => PipeError::value(pipe, source),
            StandardFuncsError::PageParseFail { message } => {
                PipeError::page_parse(pipe, PageError::ParseFail { message })
            }
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

pub struct TextFilterPageClassifyStandard;

impl TextFilterPageClassifyStandard {
    pub fn call(&self, pdf_blks: &[PdfBlock]) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let last = pdf_blks.last().ok_or(StandardFuncsError::NoPdfBlocks)?;
        let mut found: Option<&BlockValue> = None;
        for blk in pdf_blks {
            let page_type = blk.metadata_or_fail("page_type")?;
            if !page_type.is_null() {
                if let Some(existing) = found {
                    return Err(StandardFuncsError::ConflictingPageClass {
                        first: format!("{existing:?}"),
                        second: format!("{page_type:?}"),
                    });
                }
                found = Some(page_type);
            }
        }
        let page_type = found.cloned().unwrap_or(BlockValue::Null);
        let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
        Ok(vec![TextBlock::new(BlockType::PAGE_CLASS, metadata, last.clone())])
    }
}

static ISO_CODE_CANDIDATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[A-Z]{3}\b").expect("fixed, hand-written pattern, valid onig regex"));

fn word_boundary_pattern(word: &str) -> Regex {
    Regex::new(&format!(r"\b{word}\b")).expect("currency code/alias is a fixed, valid pattern")
}

/// Estrae una [`Currency`] da testo libero — vedi il doc-comment del modulo per l'algoritmo a due
/// passate (prima passata su candidate a tre lettere maiuscole nell'ordine in cui appaiono nel
/// testo cosi' com'e' scritto, seconda passata sui codici ISO/alias `EURO` nel testo
/// maiuscolizzato).
///
/// **Nota su `onig`**: `Regex::is_match` in questo crate testa un match sull'*intera* stringa
/// (vedi la sua doc "Match vs Search"), non una ricerca al suo interno — a differenza di
/// `Regex::find`/`find_iter`, che cercano ovunque. Ogni controllo qui sotto usa quindi `find`,
/// non `is_match`, per un pattern che deve poter matchare in qualunque posizione del testo.
pub fn extract_currency_from_text(text: &str) -> Result<Currency, StandardFuncsError> {
    for (start, end) in ISO_CODE_CANDIDATE.find_iter(text) {
        if let Some(currency) = Currency::from_name(&text[start..end]) {
            return Ok(currency);
        }
    }

    let upper = text.to_uppercase();
    for currency in Currency::variants() {
        if word_boundary_pattern(currency.code()).find(&upper).is_some() {
            return Ok(*currency);
        }
    }
    if word_boundary_pattern("EURO").find(&upper).is_some() {
        return Ok(Currency::EUR);
    }

    Err(StandardFuncsError::NoCurrencyFound)
}

impl TextFilterPipe for TextFilterPageClassifyStandard {
    fn name(&self) -> &str {
        "TextFilterPageClassifyStandard"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        _data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks).map_err(|e| e.into_pipe_error(self.name()))
    }
}

// ---------------------------------------------------------------------------------------------
// TextFilterInvestmentsStandard, con PdfBlocksTable inlined (è il suo unico chiamante reale)
// ---------------------------------------------------------------------------------------------

/// La tabella degli investimenti di una pagina, ricostruita dai metadati `table-row`/`table-col`
/// dei blocchi PDF.
///
/// Porting di `PdfBlocksTable` del riferimento, con una semplificazione resa possibile
/// dall'uscita da Python: là la tabella tiene **due** viste degli stessi oggetti (`_blks` piatta e
/// `_table` per riga/colonna) e conta sull'aliasing di Python perché una mutazione fatta da una
/// vista si veda dall'altra. Qui i blocchi sono valori, non riferimenti: l'aliasing non esiste e
/// non serve, quindi la vista per riga/colonna conserva **indici** nella lista piatta, che è
/// l'unica proprietaria. Il comportamento osservabile è lo stesso; sparisce la possibilità che le
/// due viste divergano.
///
/// Assunzione ereditata dal riferimento: i valori di `table-row` sono `0..n_righe`, contigui.
/// Là violarla dà un `IndexError`; qui dà [`StandardFuncsError::InconsistentTable`].
struct PdfBlocksTable {
    blks: Vec<PdfBlock>,
    /// riga → colonna → indici in `blks` che occupano quella cella (di norma 0 o 1; più d'uno è
    /// possibile ed è gestito, come nel riferimento).
    indexes: Vec<Vec<Vec<usize>>>,
}

/// Che cosa c'è in una cella.
///
/// [`Cell::Many`] non porta i blocchi: nel riferimento la cella con più blocchi restituisce la
/// lista grezza, e i suoi due soli chiamanti o guardano solo "è occupata?" (indifferente) o
/// leggono subito `.content`, che su una lista solleva `AttributeError` e viene catturato
/// ricadendo su `None`. Qui la variante senza dati esprime esattamente quelle due risposte —
/// "occupata" sì, "leggibile" no — senza tenere valori che nessuno legge.
enum Cell<'a> {
    Empty,
    One(&'a PdfBlock),
    Many,
}

/// Legge un metadato intero obbligatorio di un blocco della tabella.
fn table_meta_int(block: &PdfBlock, field: &str) -> Result<i64, StandardFuncsError> {
    Ok(block.metadata_or_fail(field)?.int_or_fail(field)?)
}

/// Legge un metadato booleano obbligatorio di un blocco della tabella.
fn table_meta_bool(block: &PdfBlock, field: &str) -> Result<bool, StandardFuncsError> {
    Ok(block.metadata_or_fail(field)?.bool_or_fail(field)?)
}

/// Il contenuto testuale di un blocco della tabella.
fn table_content(block: &PdfBlock) -> Result<String, StandardFuncsError> {
    Ok(block.content.str_or_fail("content")?.to_string())
}

impl PdfBlocksTable {
    fn new(pdf_blocks: &[PdfBlock]) -> Result<Self, StandardFuncsError> {
        let blks = pdf_blocks.to_vec();

        let mut grouped: BTreeMap<i64, BTreeMap<i64, Vec<usize>>> = BTreeMap::new();
        let mut col_max: i64 = 0;
        for (i, blk) in blks.iter().enumerate() {
            let row = table_meta_int(blk, "table-row")?;
            let col = table_meta_int(blk, "table-col")?;
            col_max = col_max.max(col);
            grouped.entry(row).or_default().entry(col).or_default().push(i);
        }

        let indexes = grouped
            .values()
            .map(|cols| {
                (0..=col_max).map(|col| cols.get(&col).cloned().unwrap_or_default()).collect()
            })
            .collect();

        Ok(PdfBlocksTable { blks, indexes })
    }

    fn len(&self) -> usize {
        self.blks.len()
    }

    fn n_cols(&self) -> usize {
        self.indexes.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// `self._blks[i]` di Python, indici negativi compresi (`-1` = ultimo).
    fn get_flat(&self, i: i64) -> Option<&PdfBlock> {
        let len = self.blks.len() as i64;
        let idx = if i < 0 { i + len } else { i };
        (0..len).contains(&idx).then(|| &self.blks[idx as usize])
    }

    /// `self._table[row][col]` di Python, indici negativi compresi; fuori range è una cella vuota,
    /// non un errore (è ciò che fa il riferimento).
    fn get_cell(&self, row: i64, col: i64) -> Cell<'_> {
        let rows = self.indexes.len() as i64;
        let r = if row < 0 { row + rows } else { row };
        if !(0..rows).contains(&r) {
            return Cell::Empty;
        }
        let row_vec = &self.indexes[r as usize];
        let cols = row_vec.len() as i64;
        let c = if col < 0 { col + cols } else { col };
        if !(0..cols).contains(&c) {
            return Cell::Empty;
        }
        match row_vec[c as usize].as_slice() {
            [] => Cell::Empty,
            [only] => Cell::One(&self.blks[*only]),
            _ => Cell::Many,
        }
    }

    /// Toglie il blocco in posizione `j` dalla lista piatta e dalla griglia, ricompattando gli
    /// indici. Se la riga resta vuota, la riga sparisce e i `table-row` delle righe successive
    /// scalano di uno.
    fn pop(&mut self, j: usize) -> Result<(), StandardFuncsError> {
        if j >= self.blks.len() {
            return Err(StandardFuncsError::InconsistentTable {
                message: format!("cannot pop block {j}: the table has {} blocks", self.blks.len()),
            });
        }
        let blk = self.blks.remove(j);
        let row_del = table_meta_int(&blk, "table-row")?;
        let col_del = table_meta_int(&blk, "table-col")?;

        let (row_idx, col_idx) = (row_del as usize, col_del as usize);
        let cell = self
            .indexes
            .get_mut(row_idx)
            .and_then(|row| row.get_mut(col_idx))
            .ok_or_else(|| StandardFuncsError::InconsistentTable {
                message: format!("block {j} claims cell ({row_del}, {col_del}), which does not exist"),
            })?;

        if let Some(position) = cell.iter().position(|&idx| idx == j) {
            cell.remove(position);
            for row in &mut self.indexes {
                for col in row.iter_mut() {
                    for idx in col.iter_mut() {
                        if *idx > j {
                            *idx -= 1;
                        }
                    }
                }
            }
        }

        if self.indexes[row_idx].iter().all(Vec::is_empty) {
            self.indexes.remove(row_idx);
            for blk in &mut self.blks {
                let row = table_meta_int(blk, "table-row")?;
                if row > row_del {
                    blk.metadata.insert("table-row".to_string(), BlockValue::Int(row - 1));
                }
            }
        }
        Ok(())
    }

    /// Fonde il contenuto dei blocchi `j` e `i` — nell'ordine in cui compaiono nella lista, non
    /// nell'ordine degli argomenti — scrivendo il risultato in `i` e togliendo `j`.
    fn merge(&mut self, j: usize, i: usize) -> Result<(), StandardFuncsError> {
        let (first, last) = if i < j { (i, j) } else { (j, i) };
        if first >= self.blks.len() || last >= self.blks.len() {
            return Err(StandardFuncsError::InconsistentTable {
                message: format!(
                    "cannot merge blocks {j} and {i}: the table has {} blocks",
                    self.blks.len()
                ),
            });
        }
        let combined =
            format!("{}{}", table_content(&self.blks[first])?, table_content(&self.blks[last])?);
        self.blks[i].content = BlockValue::Str(combined);
        self.pop(j)
    }
}

/// Dove si trova la riga che si sta estraendo: la posizione nella lista piatta, la cella
/// d'ancoraggio nella griglia e la larghezza della tabella.
///
/// I tre valori viaggiano sempre insieme (`run_loop` li calcola una volta e li passa sia a
/// `push_extracted_field` sia a `extract_field`), quindi stanno in una struct invece che in tre
/// parametri ripetuti.
#[derive(Debug, Clone, Copy)]
struct RowAnchor {
    /// Posizione della riga nella lista piatta dei blocchi.
    flat_index: i64,
    /// Cella `(riga, colonna)` da cui partono gli spiazzamenti geometrici.
    base: (i64, i64),
    /// Numero di colonne della tabella, per il rientro degli spiazzamenti.
    n_cols: i64,
}

/// Estrae le righe di investimento di una tabella, una per società bersaglio riconosciuta.
///
/// Le `*_pos` sono spiazzamenti rispetto alla cella d'ancoraggio: in modalità geometrica
/// (`geometrical_indexes`) sono distanze lineari che rientrano nella riga successiva quando
/// superano la larghezza della tabella; altrimenti sono spiazzamenti nella lista piatta dei
/// blocchi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFilterInvestmentsStandard {
    pub market_value_pos: i64,
    pub nominal_quantity_pos: Option<i64>,
    pub perc_net_assets_pos: Option<i64>,
    pub acquisition_currency_pos: Option<i64>,
    pub acquisition_cost_pos: Option<i64>,
    /// Se vero, gli spiazzamenti sono geometrici (riga/colonna con rientro); se falso, sono
    /// posizioni nella lista piatta.
    pub geometrical_indexes: bool,
    /// Se vero, una cella spezzata su due blocchi viene fusa nel blocco **precedente**; se falso,
    /// nel successivo.
    pub merge_prev: bool,
}

impl TextFilterInvestmentsStandard {
    /// I sette parametri sono quelli del riferimento, che li riceve dal CSV del repo formati; la
    /// firma resta la stessa perché è `formats_repo::structured` (M7) a costruirlo da quelle
    /// colonne.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        market_value_pos: i64,
        nominal_quantity_pos: Option<i64>,
        perc_net_assets_pos: Option<i64>,
        acquisition_currency_pos: Option<i64>,
        acquisition_cost_pos: Option<i64>,
        geometrical_indexes: bool,
        merge_prev: bool,
    ) -> Result<Self, StandardFuncsError> {
        // Verbatim dal riferimento, compreso il fatto che il controllo scatta **solo** quando
        // entrambe le posizioni opzionali sono presenti: con una sola delle due, un valore uguale
        // a `market_value_pos` non viene rifiutato.
        if let (Some(nq), Some(pna)) = (nominal_quantity_pos, perc_net_assets_pos)
            && (nq == market_value_pos || nq == pna || market_value_pos == pna)
        {
            return Err(StandardFuncsError::PositionsMustDiffer);
        }
        Ok(TextFilterInvestmentsStandard {
            market_value_pos,
            nominal_quantity_pos,
            perc_net_assets_pos,
            acquisition_currency_pos,
            acquisition_cost_pos,
            geometrical_indexes,
            merge_prev,
        })
    }

    /// Separa il nome del fondo e la valuta dai blocchi di tabella, poi estrae le righe.
    ///
    /// Quirk ereditato dal riferimento e conservato: se il ciclo sulla tabella non produce
    /// **nessuna** riga, il risultato è vuoto — anche il blocco di testo del nome del fondo, già
    /// costruito, viene scartato.
    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        target_companies: &[CompanyMatchInfos],
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let mut fund_found: Option<BlockValue> = None;
        let mut currency_found: Option<Currency> = None;
        let mut results: Vec<TextBlock> = Vec::new();
        let mut investments_blks: Vec<PdfBlock> = Vec::new();

        for blk in pdf_blks {
            if blk.type_block == BlockType::FUND_NAME {
                if fund_found.is_some() {
                    return Err(StandardFuncsError::TwoFundsInSamePage);
                }
                fund_found = Some(blk.content.clone());
                results.push(standard_fund_txt_blk(blk.clone()));
            } else if blk.type_block == BlockType::CURRENCY_STATEMENT {
                if currency_found.is_some() {
                    return Err(StandardFuncsError::TwoCurrenciesInSamePage);
                }
                let text = blk.content.str_or_fail("content")?;
                // Qui — e solo qui — una valuta non riconosciuta fa fallire la **pagina**, non il
                // documento: è il `PageParseFail` del riferimento.
                currency_found = Some(extract_currency_from_text(text).map_err(|e| {
                    StandardFuncsError::PageParseFail { message: e.to_string() }
                })?);
            } else {
                investments_blks.push(blk.clone());
            }
        }

        let mut inv = self.run_loop(&investments_blks, target_companies)?;
        if inv.is_empty() {
            return Ok(Vec::new());
        }
        let fund = fund_found.unwrap_or(BlockValue::Null);
        let currency = currency_found.map_or(BlockValue::Null, BlockValue::from);
        for txt_blk in &mut inv {
            txt_blk.metadata.insert("fund".to_string(), fund.clone());
            txt_blk.metadata.insert("currency".to_string(), currency.clone());
        }
        results.extend(inv);
        Ok(results)
    }

    /// Il ciclo sulle righe della tabella: per ogni blocco, decide se è spezzato sulla riga
    /// successiva, cerca una società bersaglio nel suo testo e, se la trova, ne estrae i campi.
    fn run_loop(
        &self,
        pdf_blocks: &[PdfBlock],
        target_companies: &[CompanyMatchInfos],
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let mut out = Vec::new();
        if pdf_blocks.is_empty() {
            return Ok(out);
        }
        let mut table = PdfBlocksTable::new(pdf_blocks)?;
        let n_cols = table.n_cols() as i64;

        let mut i: i64 = 0;
        // Deliberatamente dichiarata **fuori** dal ciclo: la coda sotto la riusa con il valore
        // che il ciclo le ha lasciato (0 se il ciclo non è mai girato), non con la colonna
        // dell'ultimo blocco. È così nel riferimento.
        let mut col: i64 = 0;

        while i < table.len() as i64 - 1 {
            let mut split = false;
            let (row, cell_width, mut content) = {
                let current = table.get_flat(i).ok_or(StandardFuncsError::InconsistentTable {
                    message: format!("block {i} disappeared from the table"),
                })?;
                col = table_meta_int(current, "table-col")?;
                (
                    table_meta_int(current, "table-row")?,
                    table_meta_bool(current, "is-max-width")?,
                    table_content(current)?,
                )
            };
            let (next_row, next_col, next_content) = {
                let next = table.get_flat(i + 1).ok_or(StandardFuncsError::InconsistentTable {
                    message: format!("block {} disappeared from the table", i + 1),
                })?;
                (
                    table_meta_int(next, "table-row")?,
                    table_meta_int(next, "table-col")?,
                    table_content(next)?,
                )
            };

            if col == next_col {
                let probe_row = if self.merge_prev { row } else { next_row };
                let mut n_full_cols = 0;
                let mut empty_adj = 0;
                for c in 0..n_cols {
                    if matches!(table.get_cell(probe_row, c), Cell::Empty) {
                        if c == col - 1 || c == col + 1 {
                            empty_adj += 1;
                        }
                    } else {
                        n_full_cols += 1;
                    }
                }
                if n_full_cols == 1 || empty_adj == 2 {
                    split = true;
                    if cell_width || content.ends_with(' ') || content.ends_with('\n') {
                        content.push_str(&next_content);
                    }
                }
            }

            if let Some(company) = self.matched_company(&content, target_companies)? {
                if split {
                    let (current_idx, next_idx) = (i as usize, (i + 1) as usize);
                    if self.merge_prev {
                        table.merge(current_idx, next_idx)?;
                    } else {
                        table.merge(next_idx, current_idx)?;
                    }
                }
                let anchor = RowAnchor { flat_index: i, base: (row, col), n_cols };
                self.push_extracted_field(&mut out, &table, anchor, &content, &company)?;
            }

            i += 1;
            if i >= table.len() as i64 - 1 {
                break;
            }
        }

        if i == table.len() as i64 - 1 {
            let (row, content) = {
                let last = table.get_flat(-1).ok_or(StandardFuncsError::InconsistentTable {
                    message: "the table is empty".to_string(),
                })?;
                (table_meta_int(last, "table-row")?, table_content(last)?)
            };
            if let Some(company) = self.matched_company(&content, target_companies)? {
                let anchor = RowAnchor { flat_index: i, base: (row, col), n_cols };
                self.push_extracted_field(&mut out, &table, anchor, &content, &company)?;
            }
        }
        Ok(out)
    }

    /// La società bersaglio riconosciuta nel testo, se ce n'è una.
    fn matched_company(
        &self,
        content: &str,
        target_companies: &[CompanyMatchInfos],
    ) -> Result<Option<String>, StandardFuncsError> {
        match_company(content, target_companies)
            .map(|found| found.map(str::to_string))
            .map_err(|e| StandardFuncsError::Match { message: e.to_string() })
    }

    /// Estrae i campi della riga e, se il blocco atteso c'era, lo accoda a `out`.
    ///
    /// Un [`StandardFuncsError::ExpectedTextBlockNotFound`] viene **assorbito** — la riga si
    /// salta, il ciclo prosegue — esattamente come il riferimento fa catturando l'eccezione
    /// omonima.
    fn push_extracted_field(
        &self,
        out: &mut Vec<TextBlock>,
        table: &PdfBlocksTable,
        anchor: RowAnchor,
        content: &str,
        company: &str,
    ) -> Result<(), StandardFuncsError> {
        match self.extract_field(table, anchor) {
            Ok(mut txt_blk) => {
                txt_blk.metadata.insert("company match".to_string(), BlockValue::from(content));
                txt_blk.metadata.insert("company".to_string(), BlockValue::from(company));
                out.push(txt_blk);
                Ok(())
            }
            Err(StandardFuncsError::ExpectedTextBlockNotFound) => Ok(()),
            Err(other) => Err(other),
        }
    }

    /// I campi di una riga di investimento, letti agli spiazzamenti configurati a partire dalla
    /// cella d'ancoraggio.
    ///
    /// In modalità geometrica lo spiazzamento è una **distanza lineare** che rientra nella riga
    /// successiva quando eccede la larghezza della tabella (`rem_euclid`/`div_euclid`), non una
    /// somma di coordinate: il ramo "tupla" del riferimento è codice morto, perché ogni `*_pos`
    /// è sempre un intero semplice.
    fn extract_field(
        &self,
        table: &PdfBlocksTable,
        RowAnchor { flat_index, base, n_cols }: RowAnchor,
    ) -> Result<TextBlock, StandardFuncsError> {
        let cell_content = |row: i64, col: i64| -> Option<String> {
            match table.get_cell(row, col) {
                Cell::One(b) => b.content.as_str().map(str::to_string),
                _ => None,
            }
        };
        let flat_content =
            |idx: i64| -> Option<String> { table.get_flat(idx)?.content.as_str().map(str::to_string) };
        let resolve = |offset: i64| -> Option<String> {
            if self.geometrical_indexes {
                let (r, c) = base;
                // `n_cols` non è mai zero: `run_loop` esce prima se la tabella è vuota, e ogni
                // riga della griglia ha almeno una colonna.
                let col_offset = (c + offset).rem_euclid(n_cols) - c;
                let row_offset = (c + offset).div_euclid(n_cols);
                cell_content(r + row_offset, c + col_offset)
            } else {
                flat_content(flat_index + offset)
            }
        };

        let anchor = if self.geometrical_indexes {
            match table.get_cell(base.0, base.1) {
                Cell::One(block) => block,
                _ => return Err(StandardFuncsError::ExpectedTextBlockNotFound),
            }
        } else {
            table.get_flat(flat_index).ok_or(StandardFuncsError::ExpectedTextBlockNotFound)?
        };

        let mut metadata = BTreeMap::new();
        // `metadata.get("manco")` del riferimento: la chiave assente vale `None`, non è un errore.
        metadata.insert(
            "manco".to_string(),
            anchor.metadata.get("manco").cloned().unwrap_or(BlockValue::Null),
        );

        let market_value =
            resolve(self.market_value_pos).ok_or(StandardFuncsError::ExpectedTextBlockNotFound)?;
        metadata.insert("market value".to_string(), BlockValue::from(market_value));

        for (pos, name) in [
            (self.perc_net_assets_pos, "% net assets"),
            (self.nominal_quantity_pos, "quantity"),
            (self.acquisition_currency_pos, "acquisition currency"),
            (self.acquisition_cost_pos, "acquisition cost"),
        ] {
            if let Some(pos) = pos {
                // A differenza di `market value`, un campo opzionale che non si trova non fa
                // fallire l'estrazione: resta `Null`, come nel riferimento.
                metadata
                    .insert(name.to_string(), resolve(pos).map_or(BlockValue::Null, BlockValue::from));
            }
        }

        let content = anchor.content.str_or_fail("content")?.replace('\n', "");
        let mut instrument = BlockType::EQUITY_TARGET;
        for pattern in PERC_REGEXES.iter() {
            if let Some(captures) = pattern.captures(&content)
                && let Some(matched) = captures.at(1)
            {
                instrument = BlockType::BOND_TARGET;
                metadata.insert("interest rate".to_string(), BlockValue::from(matched));
                break;
            }
        }
        for pattern in DATE_REGEXES.iter() {
            if let Some(captures) = pattern.captures(&content)
                && let Some(matched) = captures.at(1)
            {
                instrument = BlockType::BOND_TARGET;
                metadata.insert("maturity".to_string(), BlockValue::from(matched));
                break;
            }
        }

        Ok(TextBlock::new(instrument, metadata, anchor.clone()))
    }
}

impl TextFilterPipe for TextFilterInvestmentsStandard {
    fn name(&self) -> &str {
        "TextFilterInvestmentsStandard"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data.target_companies()).map_err(|e| e.into_pipe_error(self.name()))
    }
}

// Il prefisso `\A` su ogni pattern non è decorativo: il riferimento Python usa `re.match`, che
// tenta il match **solo** dalla posizione 0, mentre `onig::Regex::captures` cerca ovunque. Senza
// l'ancoraggio, su un contenuto come `"1,300,000.00 ITALY BTPS 3.4% ..."` (che inizia con una
// cifra, quindi `re.match` non matcha affatto il primo pattern, che pretende una lettera iniziale)
// una ricerca libera matcherebbe a partire da `"ITALY"` e produrrebbe un `interest rate` spurio.
// È un caso reale, non ipotetico: il riferimento lo documenta come regressione trovata su fixture
// vere.
static PERC_REGEXES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [r"\A[a-zA-Z].*((\d+[.,]\d+)\s*%).*", r"\A[a-zA-Z].*((\d+[.,]\d+)\s*).*"]
        .into_iter()
        .map(|p| Regex::new(p).expect("fixed, hand-written pattern, valid onig regex"))
        .collect()
});

static DATE_REGEXES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"\A.*(\d{2}[/\-.]\d{2}[/\-.]\d{4}).*",
        r"\A.*(\d{4}[/\-.]\d{2}[/\-.]\d{2}).*",
        r"\A.*(\d{2}[/\-.]\d{2}[/\-.]\d{2}).*",
        r"\A.*\s(\d{2}[/\-]\d{2})\s.*",
    ]
    .into_iter()
    .map(|p| Regex::new(p).expect("fixed, hand-written pattern, valid onig regex"))
    .collect()
});

/// Compila un pattern regex fornito da un repo formati, traducendo un pattern non valido in un
/// errore tipizzato invece di un `panic!` — a differenza dei pattern fissi di libreria sopra
/// (`PERC_REGEXES`/`DATE_REGEXES`/...), questi arrivano da configurazione esterna.
fn compile_pattern(pattern: &str) -> Result<Regex, StandardFuncsError> {
    Regex::new(pattern).map_err(|e| StandardFuncsError::InvalidPattern {
        pattern: pattern.to_string(),
        message: e.description().to_string(),
    })
}

/// I fondi visti come `Fund` **risolti** (`.name()`) negli step precedenti dello schedule, come
/// insieme di [`MatchFund`] — condiviso da `TextFilterManagmentCompanyStandard` e
/// `TextFilterAssetsStandard`.
fn resolved_funds(data: &FilterData<'_>) -> BTreeSet<MatchFund> {
    data.previous().iter().filter_map(Extracted::as_fund).filter_map(Fund::name).map(MatchFund::new).collect()
}

/// Il pipe `text_filter` per la classificazione SFDR (art. 6/8/9) di un fondo.
///
/// Vedi il doc-comment del modulo per l'algoritmo esatto: prende il **primo** blocco pdf, toglie
/// gli eventuali prefissi letterali (ancorati all'inizio della stringa, come un vero prefisso —
/// non una sostituzione di sottostringa ovunque compaia) e poi quelli a pattern, in
/// quest'ordine; verifica opzionalmente l'appartenenza all'insieme dei fondi-investimento visti
/// negli step precedenti.
pub struct TextFilterSfdrArticleStandard {
    prefix_strings: Vec<String>,
    prefix_patterns: Vec<Regex>,
    demand_investment_funds_match: bool,
}

impl TextFilterSfdrArticleStandard {
    pub fn new(
        prefix_strings: Vec<String>,
        prefix_patterns: Vec<String>,
        demand_investment_funds_match: bool,
    ) -> Result<Self, StandardFuncsError> {
        let prefix_patterns =
            prefix_patterns.iter().map(|p| compile_pattern(p)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { prefix_strings, prefix_patterns, demand_investment_funds_match })
    }

    /// I nomi (risolti) dei fondi-investimento (`Equity`/`Bond`) visti negli step precedenti.
    fn resolved_investment_funds(data: &FilterData<'_>) -> BTreeSet<MatchFund> {
        data.previous()
            .iter()
            .filter_map(|e| e.as_equity().map(|eq| &eq.data).or_else(|| e.as_bond().map(|b| &b.data)))
            .filter_map(|inv| inv.fund.resolved())
            .map(MatchFund::new)
            .collect()
    }

    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let first = pdf_blks.first().ok_or(StandardFuncsError::NoPdfBlocks)?;
        let mut fund_name = first.content.str_or_fail("content")?.to_string();

        // Prefissi letterali: `str::replace` — una sostituzione della sottostringa ovunque
        // compaia, esattamente come il riferimento (`fund_name.replace(prefix.as_str(), "")`),
        // applicati in ordine prima dei prefissi a pattern.
        for prefix in &self.prefix_strings {
            fund_name = fund_name.replace(prefix.as_str(), "");
        }
        for pattern in &self.prefix_patterns {
            fund_name = pattern.replace_all(&fund_name, "");
        }

        if self.demand_investment_funds_match {
            let known = Self::resolved_investment_funds(data);
            if !known.contains(&MatchFund::new(&fund_name)) {
                return Ok(Vec::new());
            }
        }

        Ok(vec![TextBlock::from_content(BlockType::SFDR_ARTICLE, first.metadata.clone(), fund_name)])
    }
}

impl TextFilterPipe for TextFilterSfdrArticleStandard {
    fn name(&self) -> &str {
        "TextFilterSfdrArticleStandard"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data).map_err(|e| e.into_pipe_error(self.name()))
    }
}

/// Il pipe `text_filter` per la società di gestione: cerca il **primo** blocco
/// `MANAGEMENT_COMPANY` e delega a [`standard_management_company_txt_blk`] (M4) — non lo
/// reimplementa.
pub struct TextFilterManagmentCompanyStandard;

impl TextFilterManagmentCompanyStandard {
    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let block = pdf_blks
            .iter()
            .find(|b| b.type_block == BlockType::MANAGEMENT_COMPANY)
            .ok_or(StandardFuncsError::ExpectedTextBlockNotFound)?;
        let funds = resolved_funds(data);
        Ok(vec![standard_management_company_txt_blk(block.clone(), &funds)])
    }
}

impl TextFilterPipe for TextFilterManagmentCompanyStandard {
    fn name(&self) -> &str {
        "TextFilterManagmentCompanyStandard"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data).map_err(|e| e.into_pipe_error(self.name()))
    }
}

/// Il pipe `text_filter` per il patrimonio di un fondo. Itera **tutti** i `pdf_blks` (nessun
/// filtro per `type_block`: si presume che il segmento gli passi solo `RELEVANT_BLOCK` prodotti
/// da `PdfExtractAssetsStandard`).
pub struct TextFilterAssetsStandard {
    date_regex: Option<Regex>,
    remove_from_fund_regexes: Vec<Regex>,
}

impl TextFilterAssetsStandard {
    pub fn new(
        date_regex: Option<&str>,
        remove_from_fund_regexes: Vec<String>,
    ) -> Result<Self, StandardFuncsError> {
        let date_regex = date_regex
            .map(|p| {
                let compiled = compile_pattern(p)?;
                if compiled.captures_len() != 1 {
                    return Err(StandardFuncsError::InvalidPattern {
                        pattern: p.to_string(),
                        message: format!(
                            "expected exactly one capturing group, found {}",
                            compiled.captures_len()
                        ),
                    });
                }
                Ok(compiled)
            })
            .transpose()?;
        let remove_from_fund_regexes =
            remove_from_fund_regexes.iter().map(|p| compile_pattern(p)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { date_regex, remove_from_fund_regexes })
    }

    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let known_funds = resolved_funds(data);
        let mut out = Vec::new();

        for blk in pdf_blks {
            let raw_fund = blk.metadata_or_fail("fund")?.str_or_fail("fund")?;
            let mut fund_name = raw_fund.to_string();
            for pattern in &self.remove_from_fund_regexes {
                fund_name = pattern.replace_all(&fund_name, "");
            }

            if !known_funds.contains(&MatchFund::new(&fund_name)) {
                continue;
            }

            let mut metadata = blk.metadata.clone();
            metadata.insert("fund".to_string(), BlockValue::from(fund_name));

            if let Some(date_regex) = &self.date_regex
                && let Some(date_value) = metadata.get("date").cloned()
            {
                let text = date_value.str_or_fail("date")?;
                let captured = date_regex
                    .captures(text)
                    .and_then(|caps| caps.at(1))
                    .ok_or_else(|| StandardFuncsError::DateRegexMismatch { text: text.to_string() })?;
                metadata.insert("date".to_string(), BlockValue::from(captured));
            }

            let currency_text = metadata.get("currency").ok_or_else(|| {
                StandardFuncsError::Value(crate::core::classes::value::BlockValueError::MissingField {
                    field: "currency".to_string(),
                })
            })?;
            let currency = extract_currency_from_text(currency_text.str_or_fail("currency")?)?;
            metadata.insert("currency".to_string(), BlockValue::from(currency));

            out.push(TextBlock::from_content(BlockType::RELEVANT_BLOCK, metadata, ""));
        }

        Ok(out)
    }
}

impl TextFilterPipe for TextFilterAssetsStandard {
    fn name(&self) -> &str {
        "TextFilterAssetsStandard"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data).map_err(|e| e.into_pipe_error(self.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::consts::Currency;
    use crate::core::classes::value::BlockValue;
    use crate::core::classes::{BlockType, PdfBlock};
    use std::collections::BTreeMap;

    fn page_class_block(page_type: Option<&str>) -> PdfBlock {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "page_type".to_string(),
            page_type.map(BlockValue::from).unwrap_or(BlockValue::Null),
        );
        PdfBlock::new(BlockType::new("SOME_PDF_TYPE"), metadata, "")
    }

    mod text_filter_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn no_classified_block_yields_a_null_page_type() {
            let blks = vec![page_class_block(None), page_class_block(None)];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].type_block, BlockType::PAGE_CLASS);
            assert_eq!(result[0].metadata.get("page_type"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_single_classified_block_is_reported() {
            let blks = vec![page_class_block(None), page_class_block(Some("investments"))];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(
                result[0].metadata.get("page_type"),
                Some(&BlockValue::Str("investments".to_string()))
            );
        }

        #[test]
        fn two_conflicting_classifications_is_an_error() {
            let blks =
                vec![page_class_block(Some("investments")), page_class_block(Some("other"))];
            assert!(TextFilterPageClassifyStandard.call(&blks).is_err());
        }

        #[test]
        fn an_empty_list_of_pdf_blocks_is_an_error() {
            assert!(TextFilterPageClassifyStandard.call(&[]).is_err());
        }

        #[test]
        fn the_resulting_pdf_block_is_the_last_one_in_the_list_not_the_first() {
            let first = page_class_block(None);
            let last = page_class_block(None);
            let blks = vec![first, last.clone()];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(result[0].pdf_block.as_deref(), Some(&last));
        }
    }

    mod extract_currency_from_text {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn finds_an_iso_code_isolated_in_noise() {
            assert_eq!(
                extract_currency_from_text("Value: 100 EUR total").unwrap(),
                Currency::EUR
            );
        }

        #[test]
        fn finds_a_currency_by_full_name_case_insensitively() {
            assert_eq!(extract_currency_from_text("amount in euro today").unwrap(), Currency::EUR);
        }

        #[test]
        fn finds_the_euro_alias() {
            assert_eq!(extract_currency_from_text("priced in EURO").unwrap(), Currency::EUR);
        }

        #[test]
        fn prefers_the_first_currency_mentioned_in_the_text() {
            // Pins the already-fixed reference bug: the result must depend on which code
            // actually appears first in the text, not on Currency's declaration order.
            assert_eq!(
                extract_currency_from_text("Converted from USD to EUR").unwrap(),
                Currency::USD
            );
            assert_eq!(
                extract_currency_from_text("Converted from EUR to USD").unwrap(),
                Currency::EUR
            );
        }

        #[test]
        fn errors_when_no_currency_is_present() {
            assert!(extract_currency_from_text("no currency here").is_err());
        }

        #[test]
        fn does_not_match_a_code_glued_to_other_letters() {
            // "EURUSD" is a single 6-letter word: neither "EUR" nor "USD" is a standalone,
            // word-boundary-delimited match inside it.
            assert!(extract_currency_from_text("Ticker: EURUSD").is_err());
        }

        #[test]
        fn does_not_match_a_code_glued_to_a_digit() {
            assert!(extract_currency_from_text("100EUR").is_err());
        }
    }

    // -----------------------------------------------------------------------------------------
    // M5: i due pipe visti attraverso i trait del motore, e TextFilterInvestmentsStandard
    // -----------------------------------------------------------------------------------------

    use crate::core::pipeline::{FilterData, PipeError, TextFilterPipe};
    use crate::formats_utils::text_filter::matcher::{CompanyMatchInfos, TargetCompanyInput};

    /// Un blocco di riga di tabella, con i tre metadati che `PdfBlocksTable` pretende.
    fn table_row(row: i64, col: i64, text: &str, is_max_width: bool) -> PdfBlock {
        let metadata = BTreeMap::from([
            ("table-row".to_string(), BlockValue::Int(row)),
            ("table-col".to_string(), BlockValue::Int(col)),
            ("is-max-width".to_string(), BlockValue::Bool(is_max_width)),
        ]);
        PdfBlock::new(BlockType::TABLE_BODY, metadata, text)
    }

    /// Società bersaglio costruite dal solo nome — `match_company` matcha già sul nome
    /// normalizzato, senza bisogno di regex o simboli.
    fn targets(names: &[&str]) -> Vec<CompanyMatchInfos> {
        CompanyMatchInfos::compile_from_target_companies(
            names
                .iter()
                .map(|name| TargetCompanyInput {
                    name: (*name).to_string(),
                    regexs: vec![],
                    symbols: vec![],
                    buds: vec![],
                })
                .collect(),
        )
        .expect("names without patterns always compile")
    }

    /// Il filtro nella configurazione più semplice: solo `market_value_pos`, indici geometrici.
    fn simple_investments(market_value_pos: i64) -> TextFilterInvestmentsStandard {
        TextFilterInvestmentsStandard::new(market_value_pos, None, None, None, None, true, false)
            .expect("positions are consistent")
    }

    mod page_classify_as_a_text_filter_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_pipe_name_identifies_it_in_errors() {
            assert_eq!(TextFilterPageClassifyStandard.name(), "TextFilterPageClassifyStandard");
        }

        #[test]
        fn filtering_produces_the_same_block_as_the_direct_call() {
            let blks = vec![page_class_block(Some("investments"))];
            let direct = TextFilterPageClassifyStandard.call(&blks).unwrap();
            let through_trait =
                TextFilterPageClassifyStandard.filter(&blks, &FilterData::EMPTY).unwrap();
            assert_eq!(direct, through_trait);
        }

        #[test]
        fn the_filter_data_is_ignored() {
            let blks = vec![page_class_block(Some("investments"))];
            let companies = targets(&["Acme"]);
            assert_eq!(
                TextFilterPageClassifyStandard
                    .filter(&blks, &FilterData::TargetCompanies(&companies))
                    .unwrap(),
                TextFilterPageClassifyStandard.filter(&blks, &FilterData::EMPTY).unwrap()
            );
        }

        #[test]
        fn an_empty_page_is_a_fatal_error_not_a_page_failure() {
            let err = TextFilterPageClassifyStandard.filter(&[], &FilterData::EMPTY).unwrap_err();
            assert_eq!(err.pipe(), "TextFilterPageClassifyStandard");
            assert!(!err.is_page_failure());
        }
    }

    mod investments_construction {
        use super::*;

        #[test]
        fn distinct_positions_are_accepted() {
            assert!(
                TextFilterInvestmentsStandard::new(0, Some(1), Some(2), None, None, true, false)
                    .is_ok()
            );
        }

        #[test]
        fn the_three_main_positions_must_differ_from_each_other() {
            for (mv, nq, pna) in [(0, 0, 1), (0, 1, 1), (0, 1, 0)] {
                assert!(
                    TextFilterInvestmentsStandard::new(
                        mv,
                        Some(nq),
                        Some(pna),
                        None,
                        None,
                        true,
                        false
                    )
                    .is_err(),
                    "({mv}, {nq}, {pna}) should be rejected"
                );
            }
        }

        #[test]
        fn a_single_optional_position_is_never_checked_against_market_value() {
            // Quirk verbatim del riferimento: il controllo scatta solo se *entrambe* le
            // posizioni opzionali sono presenti, quindi qui la collisione con `market_value_pos`
            // passa inosservata.
            assert!(
                TextFilterInvestmentsStandard::new(0, Some(0), None, None, None, true, false)
                    .is_ok()
            );
            assert!(
                TextFilterInvestmentsStandard::new(0, None, Some(0), None, None, true, false)
                    .is_ok()
            );
        }

        #[test]
        fn the_optional_acquisition_positions_are_never_checked() {
            assert!(
                TextFilterInvestmentsStandard::new(0, None, None, Some(0), Some(0), true, false)
                    .is_ok()
            );
        }
    }

    mod investments_field_extraction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_matched_row_becomes_one_text_block_carrying_the_company() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out.len(), 1);
            assert_eq!(out[0].metadata.get("company"), Some(&BlockValue::from("Acme Corp")));
            assert_eq!(out[0].metadata.get("company match"), Some(&BlockValue::from("Acme Corp")));
            assert_eq!(out[0].metadata.get("market value"), Some(&BlockValue::from("1.000")));
        }

        #[test]
        fn a_row_with_no_target_company_produces_nothing() {
            let blks = vec![table_row(0, 0, "Nothing here", false), table_row(0, 1, "1.000", false)];
            assert!(simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }

        #[test]
        fn the_optional_fields_are_read_at_their_offsets() {
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "1.000", false),
                table_row(0, 2, "12,5", false),
                table_row(0, 3, "42", false),
            ];
            let filter =
                TextFilterInvestmentsStandard::new(1, Some(3), Some(2), None, None, true, false)
                    .unwrap();
            let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out[0].metadata.get("% net assets"), Some(&BlockValue::from("12,5")));
            assert_eq!(out[0].metadata.get("quantity"), Some(&BlockValue::from("42")));
        }

        #[test]
        fn an_optional_field_that_is_not_there_stays_null_instead_of_failing() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            // `% net assets` punta alla colonna 5, che non esiste.
            let filter =
                TextFilterInvestmentsStandard::new(1, None, Some(5), None, None, true, false)
                    .unwrap();
            let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("% net assets"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_market_value_that_is_not_there_drops_the_whole_row() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            // A differenza dei campi opzionali, un `market value` mancante fa saltare la riga.
            assert!(simple_investments(9).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }

        #[test]
        fn the_manco_metadata_of_the_anchor_is_carried_over() {
            let mut anchor = table_row(0, 0, "Acme Corp", false);
            anchor.metadata.insert("manco".to_string(), BlockValue::from("Acme SGR"));
            let blks = vec![anchor, table_row(0, 1, "1.000", false)];

            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("manco"), Some(&BlockValue::from("Acme SGR")));
        }

        #[test]
        fn an_anchor_without_manco_gets_a_null_one() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("manco"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_geometric_offset_wraps_into_the_next_row() {
            // Tabella 2x2: dalla cella (0,0) l'offset 2 rientra nella riga successiva, colonna 0.
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "ignored", false),
                table_row(1, 0, "wrapped", false),
                table_row(1, 1, "ignored too", false),
            ];
            let out = simple_investments(2).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("market value"), Some(&BlockValue::from("wrapped")));
        }

        #[test]
        fn flat_offsets_walk_the_block_list_instead_of_the_grid() {
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "1.000", false),
                table_row(1, 0, "next row", false),
            ];
            let filter =
                TextFilterInvestmentsStandard::new(2, None, None, None, None, false, false)
                    .unwrap();
            let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("market value"), Some(&BlockValue::from("next row")));
        }
    }

    mod investments_instrument_detection {
        use super::*;
        use pretty_assertions::assert_eq;

        fn instrument_of(text: &str) -> TextBlock {
            let blks = vec![table_row(0, 0, text, false), table_row(0, 1, "1.000", false)];
            simple_investments(1)
                .call(&blks, &targets(&[text]))
                .unwrap()
                .into_iter()
                .next()
                .expect("the row matches its own text")
        }

        #[test]
        fn a_plain_name_is_an_equity() {
            let blk = instrument_of("Acme Corp");
            assert_eq!(blk.type_block, BlockType::EQUITY_TARGET);
            assert!(!blk.metadata.contains_key("interest rate"));
            assert!(!blk.metadata.contains_key("maturity"));
        }

        #[test]
        fn a_percentage_after_a_leading_letter_makes_it_a_bond() {
            let blk = instrument_of("Acme Corp 3,5 % 2030");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
            assert_eq!(blk.metadata.get("interest rate"), Some(&BlockValue::from("3,5 %")));
        }

        #[test]
        fn a_date_makes_it_a_bond_even_without_an_interest_rate() {
            let blk = instrument_of("Acme Corp mat 28/03/2025");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
            assert_eq!(blk.metadata.get("maturity"), Some(&BlockValue::from("28/03/2025")));
        }

        #[test]
        fn content_starting_with_a_digit_gets_no_spurious_interest_rate() {
            // Regressione documentata nel riferimento: i pattern delle percentuali sono ancorati
            // con `\A` e pretendono una lettera iniziale, quindi su un contenuto che inizia con
            // una cifra non devono matchare — anche se "3.4%" compare piu' avanti nel testo. Una
            // ricerca non ancorata matcherebbe a partire da "ITALY" e inventerebbe un campo.
            let blk = instrument_of("1,300,000.00 ITALY BTPS 3.4% 23-28/03/2025");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
            assert!(
                !blk.metadata.contains_key("interest rate"),
                "no spurious 'interest rate' must be produced"
            );
            assert_eq!(blk.metadata.get("maturity"), Some(&BlockValue::from("28/03/2025")));
        }

        #[test]
        fn newlines_are_stripped_before_the_patterns_are_tried() {
            let blk = instrument_of("Acme\nCorp 3,5 % 2030");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
        }
    }

    mod investments_split_cells {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Il nome della società spezzato su due blocchi consecutivi della **stessa colonna**, con
        /// la seconda metà da sola nella riga 1.
        ///
        /// "Da sola" è la condizione che fa scattare la fusione: il riferimento considera una
        /// cella spezzata solo se la riga di sondaggio ha **una sola** colonna occupata (oppure se
        /// entrambe le colonne adiacenti sono vuote). Il valore di mercato sta quindi nella riga 0,
        /// non nella riga 1, che deve restare occupata da un blocco solo.
        fn split_table(first: &str, second: &str, is_max_width: bool) -> Vec<PdfBlock> {
            vec![
                table_row(0, 0, first, is_max_width),
                table_row(1, 0, second, false),
                table_row(0, 1, "1.000", false),
            ]
        }

        #[test]
        fn a_cell_flagged_max_width_is_joined_with_the_next_one_before_matching() {
            let blks = split_table("Acme ", "Corp", true);
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(
                out[0].metadata.get("company match"),
                Some(&BlockValue::from("Acme Corp"))
            );
        }

        #[test]
        fn a_cell_ending_in_a_space_is_joined_even_without_the_max_width_flag() {
            let blks = split_table("Acme ", "Corp", false);
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out.len(), 1);
        }

        #[test]
        fn a_cell_ending_in_a_newline_is_joined_too() {
            let blks = split_table("Acme\n", "Corp", false);
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out.len(), 1);
        }

        #[test]
        fn a_cell_that_neither_is_max_width_nor_ends_in_whitespace_is_not_joined() {
            let blks = split_table("Acme", "Corp", false);
            assert!(simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }

        #[test]
        fn blocks_in_different_columns_are_never_joined() {
            let blks = vec![
                table_row(0, 0, "Acme ", true),
                table_row(0, 1, "Corp", false),
                table_row(0, 2, "1.000", false),
            ];
            assert!(simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }
    }

    mod investments_page_level {
        use super::*;
        use pretty_assertions::assert_eq;

        fn fund_block(name: &str) -> PdfBlock {
            PdfBlock::bare(BlockType::FUND_NAME, name)
        }

        fn currency_block(text: &str) -> PdfBlock {
            PdfBlock::bare(BlockType::CURRENCY_STATEMENT, text)
        }

        fn rows() -> Vec<PdfBlock> {
            vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)]
        }

        #[test]
        fn the_fund_name_becomes_its_own_text_block_before_the_rows() {
            let mut blks = vec![fund_block("Alpha Fund")];
            blks.extend(rows());
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out.len(), 2);
            assert_eq!(out[0].type_block, BlockType::FUND);
            assert_eq!(out[1].metadata.get("fund"), Some(&BlockValue::from("Alpha Fund")));
        }

        #[test]
        fn the_declared_currency_is_stamped_on_every_row() {
            let mut blks = vec![currency_block("amounts in EUR")];
            blks.extend(rows());
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("currency"), Some(&BlockValue::from(Currency::EUR)));
        }

        #[test]
        fn a_page_without_fund_or_currency_leaves_both_null() {
            let out = simple_investments(1).call(&rows(), &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("fund"), Some(&BlockValue::Null));
            assert_eq!(out[0].metadata.get("currency"), Some(&BlockValue::Null));
        }

        #[test]
        fn two_fund_names_in_the_same_page_is_an_error() {
            let blks = vec![fund_block("Alpha"), fund_block("Beta")];
            assert!(matches!(
                simple_investments(1).call(&blks, &targets(&["Acme"])),
                Err(StandardFuncsError::TwoFundsInSamePage)
            ));
        }

        #[test]
        fn two_currency_statements_in_the_same_page_is_an_error() {
            let blks = vec![currency_block("in EUR"), currency_block("in USD")];
            assert!(matches!(
                simple_investments(1).call(&blks, &targets(&["Acme"])),
                Err(StandardFuncsError::TwoCurrenciesInSamePage)
            ));
        }

        #[test]
        fn an_unreadable_currency_fails_the_page_rather_than_the_document() {
            let blks = vec![currency_block("no currency at all")];
            let err = simple_investments(1).call(&blks, &targets(&["Acme"])).unwrap_err();
            assert!(matches!(err, StandardFuncsError::PageParseFail { .. }));
            assert!(err.into_pipe_error("p").is_page_failure());
        }

        #[test]
        fn no_matched_rows_discards_the_fund_block_too() {
            // Quirk verbatim del riferimento: se il ciclo non produce righe, il risultato e'
            // vuoto — anche il blocco del fondo, gia' costruito, viene buttato.
            let mut blks = vec![fund_block("Alpha Fund")];
            blks.extend(rows());
            assert!(
                simple_investments(1).call(&blks, &targets(&["Nobody"])).unwrap().is_empty()
            );
        }

        #[test]
        fn a_page_with_no_blocks_at_all_produces_nothing() {
            assert!(simple_investments(1).call(&[], &targets(&["Acme"])).unwrap().is_empty());
        }

        #[test]
        fn a_page_with_only_a_fund_name_produces_nothing() {
            let blks = vec![fund_block("Alpha Fund")];
            assert!(simple_investments(1).call(&blks, &targets(&["Acme"])).unwrap().is_empty());
        }
    }

    mod investments_malformed_input {
        use super::*;

        #[test]
        fn a_row_without_table_coordinates_is_a_value_error() {
            let blks = vec![PdfBlock::bare(BlockType::TABLE_BODY, "Acme Corp")];
            assert!(matches!(
                simple_investments(0).call(&blks, &targets(&["Acme Corp"])),
                Err(StandardFuncsError::Value(_))
            ));
        }

        #[test]
        fn a_row_whose_content_is_not_text_is_a_value_error() {
            let metadata = BTreeMap::from([
                ("table-row".to_string(), BlockValue::Int(0)),
                ("table-col".to_string(), BlockValue::Int(0)),
                ("is-max-width".to_string(), BlockValue::Bool(false)),
            ]);
            let blks = vec![PdfBlock::new(BlockType::TABLE_BODY, metadata, 42i64)];
            assert!(matches!(
                simple_investments(0).call(&blks, &targets(&["Acme"])),
                Err(StandardFuncsError::Value(_))
            ));
        }
    }

    mod investments_as_a_text_filter_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        fn rows() -> Vec<PdfBlock> {
            vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)]
        }

        #[test]
        fn the_pipe_name_identifies_it_in_errors() {
            assert_eq!(simple_investments(1).name(), "TextFilterInvestmentsStandard");
        }

        #[test]
        fn the_target_companies_come_from_the_filter_data() {
            let companies = targets(&["Acme Corp"]);
            let out = simple_investments(1)
                .filter(&rows(), &FilterData::TargetCompanies(&companies))
                .unwrap();
            assert_eq!(out.len(), 1);
        }

        #[test]
        fn a_later_schedule_step_sees_no_target_companies_and_matches_nothing() {
            // Conseguenza diretta della semantica di `FilterData` scelta dall'utente (D-M5-1):
            // fuori dal primo step questo pipe non ha società con cui fare match.
            let previous = Vec::new();
            let out =
                simple_investments(1).filter(&rows(), &FilterData::Previous(&previous)).unwrap();
            assert!(out.is_empty());
        }

        #[test]
        fn an_unreadable_currency_becomes_a_non_fatal_page_failure() {
            let blks = vec![PdfBlock::bare(BlockType::CURRENCY_STATEMENT, "nothing here")];
            let err = simple_investments(1).filter(&blks, &FilterData::EMPTY).unwrap_err();
            assert!(err.is_page_failure());
            assert_eq!(err.pipe(), "TextFilterInvestmentsStandard");
        }

        #[test]
        fn a_malformed_row_becomes_a_fatal_value_error() {
            let blks = vec![PdfBlock::bare(BlockType::TABLE_BODY, "Acme Corp")];
            let companies = targets(&["Acme Corp"]);
            let err = simple_investments(0)
                .filter(&blks, &FilterData::TargetCompanies(&companies))
                .unwrap_err();
            assert!(matches!(err, PipeError::Value { .. }));
            assert!(!err.is_page_failure());
        }
    }

    // -----------------------------------------------------------------------------------------
    // M8: le tre funzioni deferite (`agent-memory/M8-implementation-plan.md` §2, passo 9).
    // -----------------------------------------------------------------------------------------

    mod text_filter_sfdr_article {
        use super::*;
        use crate::commons::consts::Currency as Cur;
        use crate::output::classes::investment::{Equity, InvestmentFields};

        fn sfdr_pdf_block(content: &str) -> PdfBlock {
            let metadata = BTreeMap::from([("article".to_string(), BlockValue::from("Art. 8"))]);
            PdfBlock::new(BlockType::SFDR_ARTICLE, metadata, content)
        }

        fn investment_fund(fund: &str) -> Extracted {
            Extracted::Equity(
                Equity::build(InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    BlockValue::from(fund),
                    BlockValue::from(1.0),
                    BlockValue::from(Cur::EUR),
                ))
                .unwrap(),
            )
        }

        mod construction {
            use super::*;

            #[test]
            fn empty_prefixes_are_accepted() {
                assert!(TextFilterSfdrArticleStandard::new(vec![], vec![], true).is_ok());
            }

            #[test]
            fn an_invalid_regex_prefix_is_rejected_at_construction() {
                assert!(TextFilterSfdrArticleStandard::new(vec![], vec!["(unterminated".to_string()], true).is_err());
            }
        }

        mod prefix_stripping {
            use super::*;

            #[test]
            fn a_literal_prefix_is_stripped() {
                let filter = TextFilterSfdrArticleStandard::new(vec!["Prefix: ".to_string()], vec![], true).unwrap();
                let previous = vec![investment_fund("Acme Fund")];
                let blks = vec![sfdr_pdf_block("Prefix: Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("Acme Fund"));
            }

            #[test]
            fn a_regex_prefix_is_stripped_too() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec!["^Prefix \\d+: ".to_string()], true).unwrap();
                let previous = vec![investment_fund("Acme Fund")];
                let blks = vec![sfdr_pdf_block("Prefix 42: Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("Acme Fund"));
            }

            #[test]
            fn literal_prefixes_are_applied_before_regex_prefixes() {
                // Il letterale è un `str::replace` **non ancorato** (verificato contro il
                // riferimento congelato: `fund_name.replace(prefix.as_str(), "")`), quindi rimuove
                // "Foo " ovunque compaia, non solo in testa. Con questo input l'ordine conta
                // davvero: se il letterale va per primo toglie "Foo " a metà stringa, lasciando
                // "Extra Bar" — su cui la regex ancorata "^Extra Foo " non trova più nulla da
                // togliere (il suo "Foo " è già sparito). Se la regex andasse per prima, invece,
                // matcherebbe l'intero prefisso "Extra Foo " sull'input originale e lo
                // rimuoverebbe, lasciando solo "Bar". I due ordini producono risultati diversi,
                // a dimostrazione che il letterale va davvero applicato per primo.
                let filter = TextFilterSfdrArticleStandard::new(
                    vec!["Foo ".to_string()],
                    vec!["^Extra Foo ".to_string()],
                    true,
                )
                .unwrap();
                let previous = vec![investment_fund("Extra Bar")];
                let blks = vec![sfdr_pdf_block("Extra Foo Bar")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("Extra Bar"));
            }
        }

        mod investment_fund_match {
            use super::*;

            #[test]
            fn a_fund_present_among_resolved_investments_matches() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], true).unwrap();
                let previous = vec![investment_fund("Acme Fund")];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn a_fund_not_present_produces_nothing_when_match_is_demanded() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], true).unwrap();
                let previous = vec![investment_fund("Someone Else Fund")];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn no_match_is_demanded_when_the_flag_is_false() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn an_unresolved_investment_fund_does_not_count_as_a_match() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], true).unwrap();
                let pending_equity = Extracted::Equity(
                    Equity::build(InvestmentFields::new(
                        "Acme Corp",
                        "Acme",
                        crate::core::promise::Promise::new("fund-id").into(),
                        BlockValue::from(1.0),
                        BlockValue::from(Cur::EUR),
                    ))
                    .unwrap(),
                );
                let previous = vec![pending_equity];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert!(out.is_empty());
            }
        }

        mod pdf_blocks_and_metadata {
            use super::*;

            #[test]
            fn an_empty_list_of_pdf_blocks_is_an_error() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                assert!(matches!(
                    filter.call(&[], &FilterData::Previous(&previous)),
                    Err(StandardFuncsError::NoPdfBlocks)
                ));
            }

            #[test]
            fn only_the_first_pdf_block_is_used() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("First Fund"), sfdr_pdf_block("Second Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("First Fund"));
            }

            #[test]
            fn the_metadata_of_the_first_block_is_carried_over() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].metadata.get("article"), Some(&BlockValue::from("Art. 8")));
            }

            #[test]
            fn the_result_is_typed_as_sfdr_article() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].type_block, BlockType::SFDR_ARTICLE);
            }
        }

        mod as_a_text_filter_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_errors() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                assert_eq!(filter.name(), "TextFilterSfdrArticleStandard");
            }

            #[test]
            fn an_empty_pdf_block_list_is_a_fatal_error_not_a_page_failure() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let err = filter.filter(&[], &FilterData::EMPTY).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    mod text_filter_managment_company {
        use super::*;
        use crate::output::classes::fund::Fund;

        fn manco_block(content: &str) -> PdfBlock {
            PdfBlock::bare(BlockType::MANAGEMENT_COMPANY, content)
        }

        #[test]
        fn builds_the_same_block_as_the_standard_txt_blk_helper() {
            let block = manco_block("Acme AM");
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund")), Extracted::Fund(Fund::new("Beta Fund"))];
            let via_filter = TextFilterManagmentCompanyStandard
                .call(std::slice::from_ref(&block), &FilterData::Previous(&previous))
                .unwrap();

            // `Fund::name()` normalizza e maiuscolizza (`ALPHA FUND`, non `Alpha Fund`): il set
            // atteso va derivato dagli stessi `Fund` di `previous`, non da letterali indipendenti,
            // altrimenti diverge dalla scrittura che `TextFilterManagmentCompanyStandard` produce
            // davvero in `managed_funds` (che usa `MatchFund::name()`, il nome come scritto).
            let funds: BTreeSet<MatchFund> = previous
                .iter()
                .map(|extracted| MatchFund::new(extracted.as_fund().unwrap().name().unwrap()))
                .collect();
            let expected = standard_management_company_txt_blk(block, &funds);
            assert_eq!(via_filter, vec![expected]);
        }

        #[test]
        fn no_management_company_block_is_an_expected_text_block_not_found_error() {
            let previous: Vec<Extracted> = vec![];
            let blks = vec![PdfBlock::bare(BlockType::TABLE_BODY, "irrelevant")];
            assert!(matches!(
                TextFilterManagmentCompanyStandard.call(&blks, &FilterData::Previous(&previous)),
                Err(StandardFuncsError::ExpectedTextBlockNotFound)
            ));
        }

        #[test]
        fn an_empty_list_of_pdf_blocks_is_also_an_expected_text_block_not_found_error() {
            let previous: Vec<Extracted> = vec![];
            assert!(matches!(
                TextFilterManagmentCompanyStandard.call(&[], &FilterData::Previous(&previous)),
                Err(StandardFuncsError::ExpectedTextBlockNotFound)
            ));
        }

        #[test]
        fn the_first_management_company_block_is_used_when_several_are_present() {
            let previous: Vec<Extracted> = vec![];
            let blks = vec![manco_block("First"), manco_block("Second")];
            let out =
                TextFilterManagmentCompanyStandard.call(&blks, &FilterData::Previous(&previous)).unwrap();
            assert_eq!(out[0].content.as_str(), Some("First"));
        }

        #[test]
        fn an_unresolved_fund_does_not_contribute_to_managed_funds() {
            let previous = vec![Extracted::Fund(
                Fund::from_value(&BlockValue::Promise(crate::core::promise::Promise::new("id"))).unwrap(),
            )];
            let block = manco_block("Acme AM");
            let out =
                TextFilterManagmentCompanyStandard.call(&[block], &FilterData::Previous(&previous)).unwrap();
            let managed_funds = out[0].metadata.get("managed_funds").unwrap().as_set().unwrap();
            assert!(managed_funds.is_empty());
        }

        mod as_a_text_filter_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_errors() {
                assert_eq!(TextFilterManagmentCompanyStandard.name(), "TextFilterManagmentCompanyStandard");
            }

            #[test]
            fn a_missing_management_company_block_is_a_fatal_error_not_a_page_failure() {
                let err = TextFilterManagmentCompanyStandard.filter(&[], &FilterData::EMPTY).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    mod text_filter_assets {
        use super::*;
        use crate::output::classes::fund::Fund;

        fn asset_block(fund: &str, currency: &str, date: Option<&str>) -> PdfBlock {
            let mut metadata = BTreeMap::from([
                ("fund".to_string(), BlockValue::from(fund)),
                ("currency".to_string(), BlockValue::from(currency)),
            ]);
            if let Some(d) = date {
                metadata.insert("date".to_string(), BlockValue::from(d));
            }
            PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, "")
        }

        fn known_funds() -> Vec<Extracted> {
            vec![Extracted::Fund(Fund::new("Alpha Fund"))]
        }

        mod construction {
            use super::*;

            #[test]
            fn no_date_regex_and_no_removal_patterns_are_accepted() {
                assert!(TextFilterAssetsStandard::new(None, vec![]).is_ok());
            }

            #[test]
            fn a_date_regex_with_exactly_one_capturing_group_is_accepted() {
                assert!(TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), vec![]).is_ok());
            }

            #[test]
            fn a_date_regex_with_zero_capturing_groups_is_rejected() {
                assert!(TextFilterAssetsStandard::new(Some(r"\d{2}/\d{2}/\d{4}"), vec![]).is_err());
            }

            #[test]
            fn a_date_regex_with_more_than_one_capturing_group_is_rejected() {
                assert!(TextFilterAssetsStandard::new(Some(r"(\d{2})/(\d{2})/\d{4}"), vec![]).is_err());
            }

            #[test]
            fn an_invalid_removal_pattern_is_rejected() {
                assert!(TextFilterAssetsStandard::new(None, vec!["(unterminated".to_string()]).is_err());
            }
        }

        mod fund_filtering {
            use super::*;

            #[test]
            fn a_fund_present_among_resolved_funds_produces_a_block() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn a_fund_not_present_produces_nothing_for_that_block_not_an_error() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Nobody's Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn other_blocks_still_produce_output_when_one_fund_does_not_match() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks =
                    vec![asset_block("Nobody's Fund", "EUR", None), asset_block("Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn remove_from_fund_regexes_are_applied_before_the_match() {
                let filter = TextFilterAssetsStandard::new(None, vec!["^Prefix ".to_string()]).unwrap();
                let blks = vec![asset_block("Prefix Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out.len(), 1);
                assert_eq!(out[0].metadata.get("fund"), Some(&BlockValue::from("Alpha Fund")));
            }
        }

        mod date_and_currency {
            use super::*;

            #[test]
            fn the_date_regex_extracts_its_single_capturing_group() {
                let filter = TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", Some("As of 31/12/2024"))];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].metadata.get("date"), Some(&BlockValue::from("31/12/2024")));
            }

            #[test]
            fn a_date_that_does_not_match_the_configured_pattern_is_an_error() {
                let filter = TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", Some("no date here"))];
                let err = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap_err();
                assert!(matches!(err, StandardFuncsError::DateRegexMismatch { .. }));
            }

            #[test]
            fn without_a_configured_date_regex_the_date_field_is_left_untouched() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", Some("As of 31/12/2024"))];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].metadata.get("date"), Some(&BlockValue::from("As of 31/12/2024")));
            }

            #[test]
            fn the_currency_is_extracted_from_free_text_and_typed() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "Reported in EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].metadata.get("currency"), Some(&BlockValue::from(Currency::EUR)));
            }

            #[test]
            fn an_unreadable_currency_is_an_error() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "no currency here", None)];
                assert!(filter.call(&blks, &FilterData::Previous(&known_funds())).is_err());
            }
        }

        mod result_shape {
            use super::*;

            #[test]
            fn the_result_is_a_relevant_block_with_empty_content() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].type_block, BlockType::RELEVANT_BLOCK);
                assert_eq!(out[0].content.as_str(), Some(""));
            }

            #[test]
            fn a_missing_fund_key_is_an_error() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let metadata = BTreeMap::from([("currency".to_string(), BlockValue::from("EUR"))]);
                let blk = PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, "");
                assert!(filter.call(&[blk], &FilterData::Previous(&known_funds())).is_err());
            }
        }

        mod as_a_text_filter_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_errors() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                assert_eq!(filter.name(), "TextFilterAssetsStandard");
            }

            #[test]
            fn an_unreadable_currency_is_a_fatal_error_through_the_trait_too() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "no currency here", None)];
                let err = filter.filter(&blks, &FilterData::Previous(&known_funds())).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }
}
