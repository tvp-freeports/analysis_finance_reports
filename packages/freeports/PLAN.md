# `freeports` — piano di riscrittura in Rust

Documento di progetto per la riscrittura di `packages/freeports_core` in un crate Rust
indipendente. Sorgente dei requisiti: `packages/freeports_core/riscrittura.txt`, più i target
in `analysis_finance_reports/targets/`.

Questo piano **non fa affidamento su commenti, `agent-memory/` o decisioni precedenti**: la
struttura e i costrutti sono ripensati da zero, `freeports_core` è usato solo come riferimento
di logica.

---

## 0. Scopo, perimetro, non-obiettivi

**Scopo.** Ottenere un crate `freeports` in cui *tutta* la logica è Rust nativo, senza PyO3
diffuso e senza Pydantic/pandas/pandera, ragionando prima lato Rust e solo dopo (fase
successiva, non pianificata qui) esponendo la stessa API a Python via maturin.

**Perimetro di questa fase.**

- Crate `freeports` (`lib` + `bin` omonimi) in `packages/freeports/`, che si compila e si testa
  in modo del tutto indipendente da `packages/freeports_core/`.
- Nessuno shim `py_*` / `PyStruct` per l'API Python: si resta in Rust.
- È accettato che `freeports-dev` si rompa e che i test attuali non passino durante la
  migrazione.

**Non-obiettivi (espliciti).**

- Nessun binding Python della API pubblica in questa fase.
- Nessun quarto segmento (`targets/3_add_segments.md`): il design lo rende però *economico*
  da aggiungere dopo (§5.2).
- Nessuna compatibilità binaria/pickle con i fixture esistenti.

**Da portare invariato — non ridisegnare.** L'utente ha confermato esplicitamente che questa
parte gli piace com'è implementata e va lasciata così; si porta *verbatim*, cambiando solo il
minimo indispensabile (rimozione di PyO3, adeguamento ai nomi/tipi nuovi di `PdfLine` e
`BlockValue`, aggiunta dei test mancanti):

- `formats_utils::pdf_extract::pdf_line` — i **dati** della riga PDF;
- `formats_utils::pdf_extract::select` — le **selezioni** (`select::pdf_line::{area, font,
  font_size, text}`) e le **selezioni relative** (`select::relative`, più il macchinario generico
  `OptionallyRelative` in `pdf_extract::relative`);
- `formats_utils::pdf_extract::tabularizer` (`collapse`, `coordinates`) e `position`.

Se durante il porting sembra emergere un miglioramento in questi moduli, **non applicarlo**:
segnalalo e chiedi, non riscriverlo di iniziativa.

**Da preservare quasi invariato** (stessa logica, ma libera di essere riorganizzata dove serve):
`formats_utils::text_filter`, `formats_utils::deserialize`, `commons::sets`, `commons::geometry`,
`commons::flag_expr`.

**Da ripensare interamente**: `Algorithm`, `Pipeline`, `PipelinesBundle`, i segmenti, il
caricamento del repo formati, il modello dei blocchi, la risoluzione delle promise, l'output.

---

## 1. Collocazione e build

```
analysis_finance_reports/packages/freeports/     <- nuovo crate, indipendente
analysis_finance_reports/packages/freeports_core/ <- resta com'è, intoccato
```

Crate, binario e (in futuro) modulo Python si chiamano tutti `freeports`, come richiesto.
Finché non esistono gli shim Python il crate è `crate-type = ["rlib"]`: niente `cdylib`,
niente `pyproject.toml`, niente maturin — si compila con `cargo build` e si testa con
`cargo test`. PyO3 resta comunque una dipendenza, con `auto-initialize`, perché il binario
deve incorporare un interprete per i due soli punti di contatto con Python (§3).

Comandi:

```bash
cd packages/freeports
cargo check
cargo test
cargo build --release
```

---

## 2. Principi architetturali

1. **Il core non conosce Python.** `Py<PyAny>` compare solo in tre moduli
   (`input::document`, `formats_repo::unstructured`, `formats_repo::semistructured` per il
   fallback autore). Ogni altro modulo lavora su tipi Rust.
2. **Il confine Python è un adattatore, non una dipendenza.** I pipe definiti in Python
   implementano gli stessi trait dei pipe nativi (§5.1): il resto del sistema non sa se un
   pipe è Rust o Python.
3. **Tutto ciò che attraversa un segmento è serializzabile.** `PdfBlock`/`TextBlock` e le
   classi di output derivano `Serialize`/`Deserialize`: i fixture di `freeports-dev` diventano
   JSON prodotto/consumato da serde, non pickle né Pydantic.
4. **Errori tipizzati, mai panici sul percorso utente.** Un enum d'errore per modulo con
   `thiserror`; `PageParseFail` e simili diventano varianti d'errore, non eccezioni.
5. **Logging solo `tracing`.** Nessun `logging` Python. Il file `.log.csv` diventa un
   `tracing_subscriber::Layer` che scrive righe CSV (§8).
6. **Regex con `onig` (Oniguruma), non con il crate `regex`**: i pattern nei repo formati sono
   scritti con sintassi Python/PCRE (backreference, lookaround) che `regex` non supporta.
7. **Niente pandas/pandera/polars per leggere il repo formati.** CSV letti con il crate `csv`
   in struct tipizzate + validazione esplicita. Le "join" di pandas diventano `HashMap` su
   chiavi derivate; è codice più lungo ma leggibile e con errori localizzabili alla riga.

---

## 3. I due (soli) punti di contatto con Python

| Punto | Modulo | Perché |
|---|---|---|
| Caricamento del PDF | `input::document` | PyMuPDF è lo strumento giusto e non ha equivalente Rust maturo. Si chiama `fitz` una volta per documento, si estrae il `page.get_text("dict")`, lo si converte **subito** in `core::page::Page` nativo. |
| Esecuzione dei pipe definiti dall'autore | `formats_repo::unstructured`, `formats_repo::semistructured` | I formati unstructured sono, per definizione, codice Python dell'autore del formato. |

Il `Bound<'_, PyAny>` originale della pagina viene **conservato** accanto alla `Page` nativa
(`Page::raw`), perché un pipe Python si aspetta il dizionario PyMuPDF: si converte una volta
per i pipe nativi e si passa il dict originale a quelli Python, senza riconversioni.

Gestione errori: un `PyErr` che esce da un pipe autore viene loggato con `tracing::error!`
(traceback incluso) e convertito in `PipeError::Author { pipeline, pipe, message }` al confine.
Nessun `PyErr` risale oltre `formats_repo`.

---

## 4. Modello dati del core

### 4.1 `BlockValue` (`core::classes::value`)

`metadata` e `content` devono poter contenere valori eterogenei ed essere serializzabili in
JSON. Si sceglie **un enum Rust**, non `PyAny` (che romperebbe il principio 1 e renderebbe
impossibile serde).

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum BlockValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(OrderedFloat<f64>),
    Str(String),
    Date(Date),
    Currency(Currency),
    SfdrArticle(SfdrArticle),
    FinancialInstrument(FinancialInstrument),
    Promise(Promise),
    List(Vec<BlockValue>),
    Set(BTreeSet<BlockValue>),
    Map(BTreeMap<String, BlockValue>),
}
```

Note di design:

- `OrderedFloat` (già usato altrove nel progetto) rende l'enum `Eq + Hash + Ord`, quindi
  `BlockValue` è direttamente usabile come chiave/elemento di insieme. **Sparisce così il
  trucco del `__hash__` che mutava `metadata` convertendo `set`/`list` in `frozenset`**: era
  un effetto collaterale sorprendente del Python, non serve più.
- `Set`/`Map` usano contenitori ordinati: l'hash e la serializzazione diventano deterministici.
- Accessori tipizzati (`as_str`, `as_int`, `as_promise`, `get`, `get_or_fail`) con errore
  `BlockValueError::TypeMismatch { field, expected, found }`, così i deserializer non fanno
  `unwrap`.
- Conversione da/verso Python (`FromPyObject`/`IntoPyObject`) implementata **solo** nel modulo
  di confine: se un pipe Python restituisce un oggetto non convertibile è un errore d'autore,
  non un `PyAny` opaco che si propaga.

### 4.2 `PdfBlock` / `TextBlock` (`core::classes`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockType(String);          // newtype: gli enum Python arrivano come stringa

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PdfBlock {
    pub type_block: BlockType,
    pub metadata: BTreeMap<String, BlockValue>,
    pub content: BlockValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextBlock {
    pub type_block: BlockType,
    pub metadata: BTreeMap<String, BlockValue>,
    pub content: BlockValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_block: Option<Box<PdfBlock>>,
}
```

- `type_block` è una **stringa** (newtype) e non un enum chiuso: i tipi di blocco sono
  estendibili dai repo formati, quindi un enum chiuso in libreria non funziona. Il newtype dà
  comunque type-safety rispetto a un `String` nudo e un posto dove mettere le costanti dei tipi
  standard (`BlockType::FUND`, `BlockType::CURRENCY`, ...).
- `content` è un `BlockValue` (non `String`): deve poter essere una `Promise` — è già così oggi.
- `TextBlock::from_content` resta come costruttore separato (senza `pdf_block`).
- Serializzazione: `serde_json` diretto, niente modulo `serialization.py`. `to_json`/`from_json`
  sono metodi della struct.

### 4.3 `Promise` e risoluzione (`core::promise`, `promisable`, `promise_resolution`)

Semantica invariata rispetto a oggi (suffissi `!` = strict, `[]` = multiple, con l'ordine di
strip che va verificato con test dedicati), ma:

- `PromiseMap` è `HashMap<String, Vec<BlockValue>>` nativo, non un dict Python.
- `flatten_promise_map` risolve le catene e rileva i cicli restituendo
  `Err(PromiseError::Circular { chain })` invece di sollevare. Un riferimento **pendente** (id
  assente dalla mappa) non e' invece un errore di appiattimento: la `Promise` resta al suo posto
  nella mappa appiattita, e la politica la decide `fulfill_promises` — non-strict `Dropped`,
  strict `Err(PromiseError::Unresolved)`. Confermato dall'utente il 2026-08-22 (§13); e' una
  divergenza voluta dal riferimento, che in quel caso faceva uscire un `CircularPromisesChain`
  fuorviante.
- `PromisableFields` diventa un trait con `pending()` / `resolve_field()` come oggi, ma il
  contratto di ritorno di `fulfill_promises` è espresso in Rust:

```rust
pub enum Fulfilled<T> {
    InPlace,          // risolto sul posto
    Dropped,          // promise non-strict irrisolvibile -> l'entità sparisce
    Expanded(Vec<T>), // promise `multiple` -> una copia per valore
}
```

  Questo elimina il triplo significato di `Option<Vec<_>>` del codice attuale.

### 4.4 `Page` e `Document` (`core::page`)

```rust
pub struct Document {
    pub id: DocumentId,      // nome corto o path/url, vedi targets/2_multireport_support.md
    pub format: FormatName,
    pub pages: Vec<Page>,
}

pub struct Page {
    pub number: u32,              // 1-based, come oggi
    pub size: (f32, f32),
    pub lines: Vec<PdfLine>,
    pub images: Vec<PageImage>,
    raw: Option<Py<PyAny>>,       // dict PyMuPDF, solo per i pipe Python
}
```

La rotazione delle bbox e il collasso degli span (oggi in `pdf_blks_acquire.py`) diventano
funzioni pure in `input::document`, testabili senza PyMuPDF a partire da dict costruiti a mano.

---

## 5. Il cuore: pipe, segmenti, pipeline, algoritmo

È la parte da ripensare davvero. Oggi in Rust `PipeSet` è un `Vec<Py<PyAny>>` deduplicato per
identità, e i tre segmenti sono tre `#[pyclass]` con metodi copiaincollati perché PyO3 ammette
un solo blocco `#[pymethods]`. Tutto questo sparisce.

### 5.1 I pipe sono trait, non callable

```rust
pub trait PdfExtractPipe: Send + Sync {
    fn name(&self) -> &str;
    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError>;
}

pub trait TextFilterPipe: Send + Sync {
    fn name(&self) -> &str;
    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError>;
}

pub trait DeserializePipe: Send + Sync {
    fn name(&self) -> &str;
    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError>;
}
```

Tre implementatori possibili per ciascun trait, uno per livello di specifica:

| Livello | Implementatore | Dove nasce |
|---|---|---|
| structured | struct native con parametri (`PdfExtractInvestmentsStandard`, ...) | costruite da `formats_repo::structured` leggendo CSV |
| semistructured | le stesse struct native, risolte **per nome**, oppure un `PyPipe` se il nome è definito dall'autore | `formats_repo::semistructured` |
| unstructured | `PyPdfExtractPipe`/`PyTextFilterPipe`/`PyDeserializePipe` che wrappano un callable Python | `formats_repo::unstructured` |

`name()` esiste per il logging e i messaggi d'errore: oggi un pipe che fallisce non è
identificabile.

`Send + Sync` è richiesto ora, non dopo: rende possibile la parallelizzazione per
pagina/documento senza riprogettare (i pipe Python restano serializzati dal GIL, ma il resto
scala).

### 5.2 `Segment<P>`: una sola implementazione per tre segmenti

```rust
pub struct Segment<P: ?Sized>(Vec<Arc<P>>);

pub type PdfExtractSegment = Segment<dyn PdfExtractPipe>;
pub type TextFilterSegment = Segment<dyn TextFilterPipe>;
pub type DeserializeSegment = Segment<dyn DeserializePipe>;

impl<P: ?Sized> Segment<P> {
    pub fn push(&mut self, pipe: Arc<P>);        // dedup per Arc::ptr_eq
    pub fn iter(&self) -> impl Iterator<Item = &Arc<P>>;
    pub fn is_empty(&self) -> bool;
}
impl<P: ?Sized> std::ops::Add for Segment<P> { /* unione preservando l'ordine */ }
```

Generico su `P` invece che triplicato: le semantiche di deduplicazione, unione, iterazione
sono scritte una volta sola. Aggiungere un quarto segmento (target 3) diventa: un trait nuovo,
un `type` alias, un campo in `Pipeline`.

**Ordine.** Oggi il Python usa un `set`, quindi l'ordine dei pipe è quello di hash. Qui
l'ordine è quello di inserimento, deterministico. È una differenza volontaria: rende i test
riproducibili.

### 5.3 `Pipeline` e `PipelinesBundle`

```rust
pub struct Pipeline {
    pub name: PipelineName,
    pub pdf_extract: PdfExtractSegment,
    pub text_filter: TextFilterSegment,
    pub deserialize: DeserializeSegment,
}

impl Pipeline {
    pub fn is_complete(&self) -> bool;
    pub fn apply(&self, page: &Page, data: &FilterData<'_>) -> Result<Vec<Extracted>, PipeError>;
}
impl std::ops::Add for Pipeline { /* fonde i tre segmenti — è così che structured + semistructured + unstructured si combinano */ }

pub struct PipelinesBundle(Vec<Pipeline>);
```

`Pipeline` ha ora un `name`: oggi il nome vive solo come chiave della mappa, e questo rende
i messaggi d'errore inutilizzabili.

`PipelineBundle` espone `apply`, `apply_pdf_extract`, `apply_text_filter`, `apply_deserialize`
(le tre parziali servono alla API di test di `freeports-dev`).

### 5.4 `FilterData` ed `Extracted`

Due enum che rimpiazzano il dispatch per `isinstance` su liste Python eterogenee:

```rust
pub enum FilterData<'a> {
    TargetCompanies(&'a [CompanyMatchInfos]),  // primo step dello schedule
    Previous(&'a [Extracted]),                 // step successivi
}

pub enum Extracted {
    Equity(Equity), Bond(Bond),
    Fund(Fund), FundAssets(FundAssets),
    FundSfdrClassification(FundSfdrClassification),
    FundEsgIndicator(FundEsgIndicator),
    FundRename(FundRename), FundMerge(FundMerge),
    ManagementCompany(ManagementCompany), InvestmentsManager(InvestmentsManager),
    Promises(PromiseEntries),        // il dict che i deserializer restituiscono
    PageClass(Option<PageClass>),    // output della pipeline di classificazione
}
```

Con `Extracted` il "ricomponi i risultati per tipo" di `run_documents` diventa un `match`
esaustivo che il compilatore verifica, invece di una catena di `is_instance_of`.

### 5.5 `Algorithm`

```rust
pub struct Algorithm {
    format: FormatName,
    page_classify: PipelinesBundle,
    page_class_finalizer: PageClassFinalizer,   // Identity | Python(Py<PyAny>)
    schedule: Schedule,                          // Vec<ScheduleStep>
    bundles: HashMap<PageClass, PipelinesBundle>,
}
```

API pubblica (i nomi sono quelli richiesti in `riscrittura.txt`):

| Metodo | Firma (concettuale) | Ruolo |
|---|---|---|
| `load` | `(repo: &Path, format: &FormatName) -> Result<Algorithm, LoadError>` | carica structured + semistructured + unstructured, fonde le pipeline omonime, valida schedule/mapping |
| `classify_pages` | `(&self, doc: &Document) -> Result<Vec<Option<PageClass>>, _>` | classifica le pagine di **un** documento e applica il finalizer |
| `classify_pages_multidocument` | `(&self, docs: &[Document]) -> Result<Vec<Vec<Option<PageClass>>>, _>` | il finalizer gira **per documento** (target 2) |
| `apply` | `(&self, doc: &Document, companies: &[CompanyMatchInfos]) -> Result<DocumentResults, _>` | pipeline completa su un documento |
| `apply_multidocument` | `(&self, docs: &[Document], companies: &[CompanyMatchInfos]) -> Result<Vec<DocumentResults>, _>` | classificazione per documento, **schedule sull'unione delle pagine** (target 2) |
| `apply_pdf_extract` | `(&self, page: &Page, class: &PageClass) -> Result<Vec<PdfBlock>, _>` | API di test per segmento |
| `apply_text_filter` | `(&self, page: &Page, class: &PageClass, data: &FilterData) -> Result<Vec<TextBlock>, _>` | idem |
| `apply_deserializer` | `(&self, blocks: &[TextBlock], class: &PageClass) -> Result<Vec<Extracted>, _>` | idem |

**Multi-documento nativo dal primo giorno.** `targets/2_multireport_support.md` chiede
esattamente la semantica che i nomi `*_multidocument` implicano: classificare per documento,
schedulare sull'unione. Farlo ora costa poco; retrofittarlo dopo costerebbe una seconda
riscrittura di `Algorithm`. `apply` diventa il caso particolare di `apply_multidocument` con un
solo documento — non due implementazioni.

`schedule_pages` non ricostruisce più tuple `(doc_name, page_n, page)` non tipizzate:

```rust
pub struct ScheduledPage<'a> { pub doc: &'a Document, pub page: &'a Page, pub class: PageClass }
```

Il fallimento di una pagina (`PageParseFail`) resta non fatale: si logga con
`tracing::warn!(doc, page, "pagina saltata")` e si prosegue — ma ora il warning **arriva**
davvero al log, a differenza di oggi (la gerarchia di logger Python era scollegata e i
messaggi non raggiungevano `.log.csv`).

---

## 6. Caricamento del repo formati

### 6.1 `formats_repo::structured`

Oggi: pandas + pandera, 4 CSV letti, indicizzati con `MultiIndex`, joinati, validati. Da
portare a:

1. `structured::tables` — una struct `#[derive(Deserialize)]` per CSV
   (`args.csv`, `additional_args.csv`, `deselection_lists.csv`, `partial_pipes.csv`),
   lette col crate `csv`.
2. `formats_repo::id_format` — parsing di `<formato>(<pipeline>)/<indice>` con `onig`, e
   derivazione di `(format_name, pipeline_name, pipe_index)` incluse le due politiche per
   l'indice mancante (`zero` per one-to-many, `infer` per one-to-maybe) che oggi sono
   `cumcount()` di pandas.
3. La join diventa: chiave `ComputedId = (format, pipeline, index)`, `HashMap<ComputedId, Row>`,
   e un errore esplicito se una riga di una tabella secondaria non ha corrispondenza (oggi
   `validate="one_to_one"` di pandas).
4. `structured::investments` e `structured::page_classify` costruiscono i pipe nativi con i
   parametri letti (è la parte che oggi sta in `pipelines/investments.py`).

Le validazioni pandera diventano funzioni `fn validate(&self) -> Result<(), TableError>` sulla
riga, con il numero di riga nel messaggio.

### 6.2 `formats_repo::semistructured`

Sostanzialmente il porting di quanto già esiste, ripulito da PyO3:

- `formats_mapping` legge `formats_mapping.csv`;
- `args` legge `args/{segment}.yaml` con `serde_yaml` (mantenendo la regola: chiave
  `"{format}({pipeline})"`, fallback alla chiave nuda `format` **solo** con pipeline vuota; se
  il valore è una lista, l'elemento è scelto per posizione contando i pipe già emessi per quel
  `(pipeline, segmento)`, non l'indice della riga CSV);
- `native` è il registro nome → costruttore nativo;
- se il nome non è nativo, si cerca in `local_extensions/{segment}.py` (modulo dell'autore) e
  si costruisce un `PyPipe`. Nome presente in entrambi = errore di configurazione.

### 6.3 `formats_repo::unstructured`

- `loader` — caricamento dinamico del modulo Python del formato (`importlib.util`), con la
  stessa risoluzione file/package di oggi e `templates/` aggiunto a `sys.path`.
- `py_pipe` — gli adattatori `PyPdfExtractPipe` ecc. Conversioni:
  `Page::raw` → argomento; risultato → `Vec<PdfBlock>` via `FromPyObject` su `BlockValue`.
- `compute_page_class` dell'autore diventa `PageClassFinalizer::Python`.

### 6.4 Fusione dei tre livelli

`Algorithm::load` costruisce `HashMap<PipelineName, Pipeline>` per ciascun livello e li somma
con `Pipeline + Pipeline` (§5.3), poi verifica che ogni pipeline sia completa. È l'unico punto
in cui i tre livelli si incontrano: nessun altro modulo sa che esistono.

---

## 7. Output

`output::classes` — le entità (`Equity`, `Bond`, `Fund`, `FundAssets`, `FundMerge`,
`FundRename`, `FundSfdrClassification`, `FundEsgIndicator`, `ManagementCompany`,
`InvestmentsManager`) come struct Rust con `Serialize`/`Deserialize` e `PromisableFields`.
Niente Pydantic: le validazioni di campo (`PositiveFloat`, `confloat(0,1)`) diventano
costruttori che restituiscono `Result`.

`output::files_schema` — le righe dei CSV con le loro chiavi e unicità (già oggi in Rust,
si porta quasi invariato).

`output::routines` — assemblaggio `Extracted` → tabelle → CSV. Decisione: **eliminare polars**
e scrivere i CSV con il crate `csv`. L'unica join reale (investments ↔ bond info) è una lookup
su `HashMap`; polars è una dipendenza pesante per quel poco.

Colonna `report` sempre presente (anche fuori dalla modalità batch) e niente `prefix out`,
come chiede `targets/2_multireport_support.md`: uniforma gli schemi output batch/non-batch.

---

## 8. Errori e logging

**Errori.** Un enum per modulo con `thiserror`, e una gerarchia esplicita:

```
PipeError            -> fallimento di un singolo pipe (Author | Extraction | Cast | ...)
PageError            -> PageParseFail equivalente, non fatale: la pagina si salta
LoadError            -> caricamento repo formati (CSV, YAML, moduli Python)
ConfigError          -> CLI/env/file
OutputError          -> scrittura risultati
```

`PageParseFail`, `LineParseFail`, `ExtractionFieldFail`, `ExpectedPdfBlockNotFound`,
`ExpectedTextBlockNotFound` — oggi eccezioni Python — diventano varianti di `PipeError`/`PageError`.

**Logging.** Solo `tracing`. Tre destinazioni configurate in `core::tracing_setup`:

1. stderr, verbosità da `-v`/`-vv`/`-vvv`;
2. `freeports.log`, filtro `debug`;
3. `.log.csv`, implementato come `Layer` custom che intercetta gli eventi con i campi
   `page`/`company`/`field`/`row`/`column` e li scrive come riga CSV.

I "contextual infos" (pagina corrente, batch, investment) diventano `tracing::span!` con campi:
niente stato globale mutabile come oggi (`LOG_CONTEXTUAL_INFOS`).

---

## 9. API pubblica

Superficie da esporre (da `riscrittura.txt`), realizzata come modulo `api` con sole re-export;
l'albero interno resta libero di cambiare.

```
cli::{CliArgs, execute}
consts::{Currency, SfdrArticle, FinancialInstrument}
core::{PdfBlock, TextBlock, Pipeline, Promise, Algorithm}
utils::pdf_extract::{pdfline_selection_from_dict, pdfline_selection_from_str,
                     pdfimages_from_pagedict, pdflines_from_pagedict,
                     get_groups, get_table_coordinates,
                     CellGeometry, SplittingState, NullableState, Limits,
                     RowConfig, ColumnConfig, TableConfig,
                     CollapseAlgorithm, TablePosAlgorithm, TablePosMeasureUnit}
utils::text_filter::{normalize_string, investment_fund_filter_data, extract_currency_from_text}
utils::deserialize::{perc_to_float, to_int, to_float, to_str, to_currency, to_date,
                     to_int_en_month, to_date_with_en_month, to_int_it_month, to_date_with_it_month}
standard_builders::text_blocks::{standard_management_company_txt_blk,
                                 standard_investmet_manager_txt_blk, standard_fund_txt_blk}
standard_builders::pdf_blocks::{}                  // vuoto oggi, il modulo esiste come segnaposto
standard_funcs::pdf_extract::{PdfExtractPageClassifyStandard, PdfExtractInvestmentsStandard,
                              PdfExtractCurrencyStandard, PdfExtractCurrencyConstant,
                              PdfExtractFundStandard, PdfExtractManagmentCompanyStandard,
                              PdfExtractSfdrArticleStandard, PdfExtractAssetsStandard}
standard_funcs::text_filter::{TextFilterPageClassifyStandard, TextFilterInvestmentsStandard,
                              TextFilterManagmentCompanyStandard, TextFilterSfdrArticleStandard,
                              TextFilterAssetsStandard}
standard_funcs::deserialize::{DeserializerPageClassifyStandard, DeserializerInvestmentStandard,
                              DeserializerFundStandard, DeserializerManagmentCompanyStandard,
                              DeserializerInvestmentsManagerStandard,
                              DeserializerInvestmentsManagerFromManco,
                              DeserializeSfdrArticleStandard, DeserializerAssetsStandard}
input::{load_target_companies, compile_target_companies}
output::{Bond, Equity, Fund, FundAssets, FundMerge, FundChangeName, SfdrArticle, FundEsgIndicators}
```

Regola: **niente `pub` verso l'esterno se non passa da `api`**. I moduli interni sono
`pub(crate)` salvo dove servono ai test di integrazione.

---

## 10. Stile dei test

**Regola generale, valida per tutto il crate: i test sono raggruppati per argomento tramite
sottomoduli.** Nessun elenco piatto di `#[test]` dentro `mod tests`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod construction {
        use super::*;
        #[test] fn rejects_negative_market_value() { /* ... */ }
        #[test] fn accepts_promise_as_fund_name() { /* ... */ }
    }

    mod serde_roundtrip {
        use super::*;
        #[test] fn nested_maps_survive_json() { /* ... */ }
    }

    mod hashing {
        use super::*;
        #[test] fn set_order_does_not_affect_hash() { /* ... */ }
    }
}
```

Criteri:

- **Un sottomodulo per argomento**: unità testata (funzione, costruttore, metodo) oppure
  invariante trasversale (`serde_roundtrip`, `error_cases`, `edge_geometry`). Sottomoduli
  annidati quando l'argomento è ampio.
- **Esaustività**: ogni variante di enum, ogni ramo di `match`, ogni ramo di errore ha almeno
  un test. Per le funzioni di parsing/cast, tabelle di casi con `test_case`.
- **Stress test** dove la logica è combinatoria (algebra degli insiemi, selezioni relative,
  tabularizer): input generati in loop, invarianti verificate (idempotenza, commutatività,
  De Morgan, `contains` coerente con l'AST semplificato).
- **Nomi dei test = comportamento atteso**, non nome del metodo:
  `resolves_last_value_when_promise_is_not_multiple`, non `test_fulfill`.
- **Test di integrazione** in `tests/`, un file per flusso: `tests/algorithm_end_to_end.rs`,
  `tests/formats_repo_loading.rs`, `tests/cli_config.rs`. Usano un repo formati minimale
  costruito in un `tempfile::TempDir`, non fixture esterni.
- **Niente Python nei test unitari.** Solo i test di `formats_repo::unstructured` e
  `input::document` possono attaccarsi all'interprete, e sono marcati e isolati in
  sottomoduli `mod python_boundary`.
- TDD: i test si scrivono prima dell'implementazione, milestone per milestone, e non si
  modificano per farli passare.

---

## 11. Milestone

L'ordine è dal basso verso l'alto (prima i moduli con meno dipendenze), come da
`targets/1_rust_rewrite.md`. Ogni milestone deve chiudersi con `cargo test` verde.

| # | Milestone | Contenuto | Focus dei test |
|---|---|---|---|
| **M0** | Scaffolding | `Cargo.toml`, albero moduli, `tracing_setup`, tipi d'errore base | compila; il layer `.log.csv` scrive righe attese |
| **M1** | `commons` | `date`, `geometry`, `sets` (3 sottomoduli), `consts`, `flag_expr`, `i18n` | algebra insiemi esaustiva + stress; parsing date; ogni valuta |
| **M2** | `core` dati | `classes` + `value`, `promise`, `promisable`, `promise_resolution`, `normalization`, `match_fund` | roundtrip serde; hash/eq; catene di promise incluse quelle circolari |
| **M3** | `pdf_extract` | `pdf_line`, `relative`, `select/*`, `position`, `tabularizer/*`, `commons` | selezioni combinatorie; geometrie degeneri; tabelle irregolari |
| **M4** | `text_filter` + `deserialize` | `matcher`, `standard_funcs` (x2), `standard_txt_blk_builders`, `cast` (scope reale in corso, spaccato fra autosufficiente e deferito a M5/M8 — vedi `STATUS.md`) | matching societario; ogni cast con casi limite e localizzazioni |
| **M5** | **motore** | `pipeline/segment`, `pipeline/bundle`, `schedule`, `algorithm` | dedup e ordine dei pipe; schedule multi-documento; pagina che fallisce |
| **M6** | `input::document` | PyMuPDF → `Page`, rotazione bbox, collasso span, immagini | funzioni pure su dict costruiti a mano; un test di confine con PyMuPDF |
| **M7** | `formats_repo` | `id_format`, `metadata`, `orchestration`, `structured/*`, `semistructured/*`, `unstructured/*` | ogni CSV malformato dà l'errore giusto con la riga giusta; fusione dei 3 livelli |
| **M8** | `output` | `classes`, `files_schema`, `routines` | promesse risolte prima della scrittura; CSV byte-per-byte su casi noti |
| **M9** | `cli` | `config_locations/*`, `partial_config`, `conf_parse`, `freeports_config`, `batch`, `job`, `run`, `main` | precedenza cmd > env > file; parsing `<url>:<path>:<name>` (vedi `targets/conf_parse.md`) |
| **M10** | Chiusura | rimozione dei residui, confronto output con `freeports_core` su un formato reale, benchmark, docs | end-to-end su repo formati reale |

Dipendenze fra milestone: M1 → M2 → {M3, M4} → M5 → {M6, M7} → M8 → M9 → M10.
M6 e M7 sono i due punti PyO3 e possono procedere in parallelo.

---

## 12. Decisioni prese (e perché)

| # | Decisione | Motivazione |
|---|---|---|
| D1 | `metadata`/`content` sono un enum Rust (`BlockValue`), non `PyAny` | serde, hash deterministico, nessun PyO3 nel core |
| D2 | `type_block` è una stringa in un newtype, non un enum chiuso | i tipi di blocco li estendono i repo formati |
| D3 | Sparisce il `__hash__` che mutava `metadata` | era un effetto collaterale; con `BTreeMap`/`BTreeSet` non serve |
| D4 | I pipe sono trait object `Arc<dyn ...>`, i segmenti sono `Segment<P>` generico | una sola implementazione invece di tre; quarto segmento economico |
| D5 | Ordine dei pipe = inserimento (non `set` come in Python) | determinismo e test riproducibili |
| D6 | `Pipeline` e i pipe hanno un `name` | messaggi d'errore utilizzabili |
| D7 | Multi-documento nativo da subito (`apply_multidocument`) | è già nella API richiesta e nel target 2; retrofittarlo dopo costa una seconda riscrittura |
| D8 | Niente pandas/pandera/**polars**: solo il crate `csv` + validazione esplicita | dipendenze pesanti per join banali; errori localizzati alla riga |
| D9 | `onig` invece di `regex` | i pattern dei repo formati usano sintassi PCRE |
| D10 | `thiserror` come unica nuova dipendenza "di comodo" | elimina centinaia di righe di `impl Display`/`From` scritte a mano |
| D11 | `Date` scritto a mano in `commons::date`, niente `chrono` | servono solo parse/format/confronto, nessuna aritmetica di calendario |
| D12 | `crate-type = ["rlib"]` finché non ci sono gli shim | build e test più veloci, nessun vincolo maturin in questa fase |
| D13 | I test unitari non toccano Python, salvo i due moduli di confine | i test restano veloci e deterministici |
| D14 | `tabularizer`, `pdf_line`, `select` (incl. `relative`) si portano **verbatim** | l'utente ha usato a lungo quel codice e ne è soddisfatto: ridisegnarlo sarebbe rischio senza guadagno (vedi §0) |

---

## 13. Punti confermati e domande ancora aperte

### Confermato dall'utente

| Punto | Esito |
|---|---|
| Nome cartella | `packages/freeports/` va bene, resta questo |
| `commons::date` e `core::page` | aggiunta approvata, restano dove sono |
| `pdf_extract::pdf_line` vs `select::pdf_line` vs `select::relative` | interpretazione corretta: **dati** / **selezioni** / **selezioni relative** |
| `tabularizer`, `pdf_line`, `select` (incl. `relative`) | **si portano com'è**, non si ridisegnano (vedi §0) |
| `int_value()` di `FinancialInstrument`/`SfdrArticle` | confermato: **non serve**, resta omesso definitivamente (2026-08-22) |
| `Set::Universe / _` in `commons::sets` | confermato: **il panic non tipizzato va bene così**, resta un limite documentato, non un target futuro (2026-08-22) |
| Riferimento promise pendente (id assente dalla mappa) | confermato: **la promise passa**. `flatten` la lascia irrisolta e non e' un errore; decide `fulfill_promises` (non-strict ⇒ `Dropped`, strict ⇒ `Unresolved`). `Circular` resta riservato ai cicli veri (2026-08-22) |
| `select::pdf_line::text` — regex vs matching semplice | confermato: matching verbatim ad ancore `^`/`$` (prefisso/suffisso/sottostringa/esatto) come `TextAstLeaf` del riferimento, **niente** `onig`. Un vecchio doc-comment di stub parlava di "regex onig" per errore (2026-08-23) |
| `CellGeometry` duplicato (position vs tabularizer) | confermato: **un solo tipo canonico**, in `tabularizer::coordinates` (quello validato, usato davvero dall'algoritmo); `position::{RowConfig,ColumnConfig,TableConfig}` non ne avevano comunque bisogno (2026-08-23) |
| `pdf_extract::relative` — genericizzare `RelativeInfo`/`OptionallyRelative` oltre `PdfLine`? | confermato: **no**, resta agganciato a `&[PdfLine]` come nel riferimento; è solo spostato di un livello da `select::relative` a `pdf_extract::relative` (2026-08-23) |
| `PdfLine.font` — tipo dato normalizzato a costruzione o stringa grezza? | confermato: **normalizzato a costruzione** (`Font`, per le stesse ragioni di performance del riferimento), ma definito in `pdf_line.rs` (dati) e non in `select::pdf_line::font` (selezioni): gli impl di selezione (`Container`/`Overlappable`/`AtomOperations`, `FontSet`) restano in `select::pdf_line::font`, che importa `Font` da `pdf_line` — lecito in Rust, la posizione di un `impl` non è vincolata a quella del tipo (2026-08-23) |
| `collapse_table_rows` — panic su `indexes` vuoto senza config colonne (ereditato verbatim dal riferimento) | confermato: **si accetta e si documenta**, stesso trattamento di `Set::Universe / _` in M1 — limite noto, non un target per M3. Da rivedere quando M5 collega `collapse_table_rows` a dati di pagina reali (2026-08-23) |
| `pub` vs `pub(crate)` sull'albero interno (da M0, non introdotto da M3) | confermato: **si lascia com'è per ora**, annotato come voce trasversale alle milestone da affrontare a parte (es. pulizia M10), non da correggere dentro M3 (2026-08-23) |

### Ancora da decidere

1. **`FilterData`** — ho mantenuto la semantica attuale (primo step dello schedule = target
   companies, step successivi = risultati precedenti, mai entrambi contemporaneamente). Va bene,
   o i pipe devono vedere sempre entrambe le cose? *Blocca M5.*
2. **Fixture di `freeports-dev`** — sono pickle Python; con serde diventano JSON e vanno
   rigenerati. Confermi, e in quale milestone (M8 o M10)? *Blocca M10, non prima.*
3. **`.log.csv`** — deve continuare a esistere con le stesse colonne
   (`Page,Matched Company,Company,Field name,Row,Column,Message`)? *Blocca M0 (il `Layer`), ma
   si può implementare con le colonne attuali e cambiarle dopo senza costi.* **Implementato con
   queste colonne in M0** (2026-08-22): riga scritta solo se l'evento/span porta almeno uno dei
   campi `page`/`company`/`field`/`row`/`column`; `Matched Company` resta sempre vuota (nessun
   campo tracing la alimenta ancora) — non blocca più, ma resta da confermare se `Matched
   Company` debba ricevere un campo dedicato in una milestone futura.
4. **`TablePosMeasureUnit`** (§9, superficie pubblica di `utils::pdf_extract`) — nessun
   riferimento esiste in `freeports_core` per questo tipo, ne' in nessun punto raggiunto dallo
   scope reale di M3. Gap fra §9 e lo scope effettivo, non un'omissione silenziosa: lasciato non
   implementato, `api::utils::pdf_extract` (M3) non lo riesporta. *Non blocca M3; da chiarire
   prima che qualche milestone futura ne dipenda davvero.*
5. **`pub` vs `pub(crate)` sull'intero albero interno** (`commons`, `core`, `formats_utils`, ...,
   `lib.rs`) — da M0 tutto e' `pub mod`, non `pub(crate) mod` come richiesto da §14: i tipi interni
   sono raggiungibili da fuori crate col percorso completo, bypassando la superficie curata di
   `api`. Non e' stato introdotto da M3 (che si limita a continuare il pattern esistente), ma e'
   una voce trasversale a tutte le milestone finora. *Non blocca nessuna milestone corrente;
   da affrontare come task a parte (candidato: pulizia M10), non dentro una singola milestone.*
Regola generale: se durante l'implementazione emerge una decisione di design non coperta da
questo documento, **si chiede all'utente** e si annota la risposta qui in §13, non la si decide
di iniziativa.

---

## 14. Come lavorare a questo piano

Questa sezione serve a chi (agente o persona) apre il progetto senza avere in mano la
conversazione in cui il piano è nato.

### Prima di scrivere una riga di codice

Leggere, in quest'ordine:

1. questo file, per intero — non solo la milestone di turno;
2. `STATUS.md` (stessa cartella), per sapere dove si è arrivati e cosa è stato deciso strada facendo;
3. `packages/freeports_core/riscrittura.txt` — le parole originali dell'utente sull'albero dei
   moduli e sulla API pubblica; questo piano ne è l'elaborazione, non lo sostituisce;
4. `analysis_finance_reports/AGENTS.md`, sezione "Architecture Overview", per il dominio
   (classificazione pagine → schedule → tre segmenti → promise);
5. il codice di `packages/freeports_core/` **solo come riferimento di logica**. I commenti e i
   documenti in `agent-memory/` di quel package descrivono un design precedente e in parte
   superato: leggerli come ispirazione, mai come vincolo.

### Regola d'oro sul codice esistente

`freeports_core` non si tocca: resta congelato per tutta la migrazione, serve da riferimento e da
termine di paragone per l'output finale (M10). Tutte le nuove feature (target 2 e 3) entrano solo
nel crate nuovo.

### Ciclo di lavoro per milestone

Una milestone è l'unità di lavoro. Per ciascuna:

1. **Requisiti** — rileggere la riga corrispondente in §11 e le sezioni di design collegate.
   Se qualcosa non è specificato, chiedere (§13, regola generale).
2. **Test prima** — scrivere i test seguendo lo stile di §10 (sottomoduli per argomento,
   esaustività su varianti/rami/errori). I test sono il contratto: non si modificano per farli
   passare.
3. **Implementazione** — far passare i test. Per i moduli "da portare invariato" (§0) il lavoro
   è un porting, non una riscrittura.
4. **Chiusura** — `cargo test` verde e `cargo clippy` senza warning nuovi; aggiornare `STATUS.md`
   (cosa è fatto, cosa è stato deciso, cosa resta aperto); abilitare in `src/lib.rs` le re-export
   di `api` che la milestone rende disponibili (§9).

Una milestone non si considera chiusa se lascia `todo!()` o test ignorati.

### Ordine e parallelismo

`M1 → M2 → {M3, M4} → M5 → {M6, M7} → M8 → M9 → M10`.
M3 e M4 sono indipendenti fra loro; M6 e M7 sono i due punti di contatto con Python e possono
procedere in parallelo. Tutto il resto è sequenziale: non anticipare M5 prima che M2 sia chiusa,
perché il motore è costruito sopra `BlockValue`/`Extracted`.

### Convenzioni di codice

- Edizione 2024. Niente `unwrap`/`expect` fuori dai test, salvo invarianti dimostrate con un
  commento accanto.
- Un enum d'errore per modulo, con `thiserror`; mai `Box<dyn Error>` nella API pubblica.
- Documentazione: `//!` sul modulo che spiega *perché* esiste e come si incastra; `///` sugli
  item pubblici. I commenti spiegano le decisioni non ovvie, non ripetono il codice.
- Niente `pub` verso l'esterno che non passi da `api` (§9); il resto è `pub(crate)`.
- PyO3 solo nei tre moduli di confine (§3). Un `use pyo3` altrove è un errore di design, non una
  scorciatoia.

---

## 15. Rischi e criticità

| Rischio | Impatto | Mitigazione |
|---|---|---|
| Il confine con i pipe Python è la parte più fragile (conversioni, errori, GIL) | alto | isolato in due moduli, con test di confine dedicati; `BlockValue` come contratto esplicito invece di `PyAny` |
| Le validazioni pandera non sono documentate: riportarle a mano può perdere controlli | medio | M7 parte dalla lettura riga per riga degli schema Python, con un test per ogni check |
| Rigenerare i fixture nasconde regressioni | medio | M10 confronta l'output CSV con quello di `freeports_core` su un formato reale, prima di rigenerare |
| Il rewrite è lungo e i due crate divergono nel frattempo | medio | `freeports_core` resta congelato durante la migrazione; le nuove feature (target 2, 3) entrano solo nel nuovo crate |
| Superficie API pubblica grande da riesportare a mano | basso | modulo `api` costruito incrementalmente, una riga per milestone |
