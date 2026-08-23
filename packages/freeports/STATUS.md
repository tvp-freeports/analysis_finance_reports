# `freeports` — stato della riscrittura

File di continuità fra sessioni. **Va aggiornato alla chiusura di ogni milestone**, prima di
considerare il lavoro finito. Il piano è in `PLAN.md`; qui c'è solo *dove siamo*.

## Stato per milestone

| # | Milestone | Stato | Note |
|---|---|---|---|
| M0 | Scaffolding | ✅ chiusa | `tracing_setup` (stderr/file/`.log.csv` layer, `TracingSetupError`) implementato e testato |
| M1 | `commons` | ✅ chiusa | `date`, `geometry`, `sets` (+ `ast_simple`/`ast_smart`/`indipendent_atoms`), `consts`, `flag_expr`, `i18n`; 407 test verdi, `cargo clippy` pulito; `api::consts` abilitato in `lib.rs` |
| M2 | `core` dati | ✅ chiusa | `normalization`, `promise`, `classes`(+`value`), `promise_resolution`, `promisable`, `match_fund`; 617 test verdi, `cargo clippy` pulito; `api::core` abilitato in `api.rs` |
| M3 | `pdf_extract` | ✅ chiusa | `pdf_line`, `relative`, `select::{pdf_line,relative}` (+`pdf_line::{area,font,font_size,text}`), `tabularizer::{collapse,coordinates}`, `position`, `commons`; 969 test verdi, `cargo clippy` pulito salvo un warning inevitabile in test verbatim (vedi sotto); `api::utils::pdf_extract` abilitato in `api.rs` per la parte già pronta |
| M4 | `text_filter` + `deserialize` | 🟡 in corso | Fatto: `deserialize::cast`, `text_filter::matcher`, `text_filter::standard_txt_blk_builders`, `TextFilterPageClassifyStandard`, `extract_currency_from_text`, `DeserializerPageClassifyStandard` (M4) e `TextFilterInvestmentsStandard` + `PdfBlocksTable` (aggiunto a chiusura di M5). **Restano deferite solo le 10 funzioni che dipendono da `output::classes`** (M8): nessuna aspetta più il motore |
| M5 | Motore (pipeline/algorithm) | ✅ chiusa | `core::page`, `core::schedule`, `core::pipeline::{data,segment,bundle}`, `core::pipeline::Pipeline`, `core::algorithm`; 1365 test unitari + 10 d'integrazione (`tests/algorithm_end_to_end.rs`) verdi, `cargo clippy --all-targets` senza warning nuovi; `api::core` estesa con `Pipeline`/`Algorithm` e il resto del motore. `Algorithm::load` resta a M7 (legge il repo formati) |
| M6 | `input::document` | ✅ chiusa | `input::document::{page_dict, selection}` (+ modulo radice); 1417 test unitari + 10 d'integrazione verdi, `cargo clippy --all-targets` senza warning nuovi; `api::utils::pdf_extract` estesa con le 4 funzioni di §9 rimaste da M3, nuovo `api::input` |
| M7 | `formats_repo` | ⬜ da fare | |
| M8 | `output` | ⬜ da fare | |
| M9 | `cli` | ⬜ da fare | |
| M10 | Chiusura e confronto | ⬜ da fare | |

Legenda: ⬜ da fare · 🟡 in corso · ✅ chiusa (test verdi, `STATUS.md` aggiornato)

## Decisioni prese durante l'implementazione

*(Ogni volta che l'utente risponde a una domanda aperta, o si prende una decisione non prevista
da `PLAN.md`, va annotata qui con la data e riportata nella sezione giusta di `PLAN.md`.)*

- 2026-08-22 — Cartella `packages/freeports/` confermata; `commons::date` e `core::page`
  confermati; `pdf_line` = dati, `select::pdf_line` = selezioni, `select::relative` = selezioni
  relative.
- 2026-08-22 — `tabularizer`, `pdf_line` e le selezioni (anche relative) si portano **com'è**:
  l'utente è soddisfatto di quel design e non vuole che venga ripensato.
- 2026-08-22 — M0 e M1 chiuse (TDD: test prima, poi implementazione, per ogni modulo). Decisioni
  di design prese durante l'implementazione (dettagli in `PLAN.md` §13):
  - `.log.csv`: colonne confermate (`Page,Matched Company,Company,Field name,Row,Column,Message`);
    riga scritta solo se l'evento (o uno span attivo che lo contiene) porta almeno uno dei campi
    `page`/`company`/`field`/`row`/`column`; colonna `Matched Company` resta sempre vuota in M0
    (nessun campo tracing la alimenta ancora).
  - `SfdrArticle`: varianti rinominate `Art6`/`Art8`/`Art9` (da `ART_6`/...) per restare puliti
    su `non_camel_case_types` — l'ordine di dichiarazione (6, 8, 9) non cambia.
  - `Currency` serializza come stringa JSON nuda (il codice ISO); la deserializzazione usa la
    semantica *esatta* di `from_code` (rifiuta l'alias `"EURO"`, che `from_name` invece accetta
    come lookup separato).
  - `Date` (`commons::date`, tipo nuovo senza riferimento diretto, `PLAN.md` §12 D11): serializza
    come stringa canonica `YYYY-MM-DD` (non come struct `{year,month,day}`); `year` vincolato a
    `0..=9999` per tenere `Display`/`FromStr` totali.
  - `commons::sets`: nessun tipo d'errore `thiserror` introdotto (il modulo è totale/infallibile,
    salvo `Set::Universe / _` che resta un panic non tipizzato, come nel riferimento — non c'è
    un modo generico di enumerare "tutto ciò che non è in un `Container`").
- 2026-08-22 — Utente confermato su entrambe le domande aperte di M1: `int_value()` di
  `FinancialInstrument`/`SfdrArticle` non serve, resta omesso definitivamente; il panic non
  tipizzato di `Set::Universe / _` va bene così, resta un limite documentato (non un target
  futuro). Riportato in `PLAN.md` §13.
- 2026-08-22 — M2 chiusa (TDD: test prima, poi implementazione, modulo per modulo). `core::page`
  **non** fa parte di M2: `PLAN.md` §11 lo colloca in M6, e dipende da `PdfLine` (M3). Decisioni
  di design prese durante l'implementazione:
  - **Riferimenti promise pendenti — risposta dell'utente a una domanda aperta sollevata in M2.**
    Una promessa che punta a un id assente dalla mappa **non è un errore di appiattimento**:
    `PromiseMap::flatten` la lascia dov'è come `BlockValue::Promise`, e la politica la decide a
    valle `fulfill_promises` (non-strict ⇒ `Fulfilled::Dropped`, strict ⇒
    `Err(PromiseError::Unresolved)`). `PromiseError::Circular` resta riservato ai cicli veri.
    È una divergenza voluta da `freeports_core`, dove il fallback `mapping.get(id, [value])`
    faceva uscire un `CircularPromisesChain` fuorviante anche per il riferimento pendente.
  - `BlockValue` deriva anche `PartialOrd`/`Ord`: `PLAN.md` §4.1 elenca `BTreeSet<BlockValue>` fra
    le varianti, che lo richiede. Di conseguenza `commons::consts::{Currency, SfdrArticle,
    FinancialInstrument}` (M1) hanno ricevuto gli stessi due derive — unica modifica a codice M1,
    puramente additiva. L'ordine è quello di dichiarazione, non alfabetico.
  - `PromiseMap`/`FlatPromiseMap` usano `BTreeMap` e non `HashMap` come diceva `PLAN.md` §4.3:
    l'appiattimento visita gli id in ordine, quindi la catena riportata da un ciclo è
    deterministica e i messaggi d'errore sono riproducibili nei test.
  - `BlockType` è un newtype su `Cow<'static, str>` e non su `String`: è ciò che rende i tipi
    standard vere costanti associate (`BlockType::FUND`), come chiede `PLAN.md` §4.2, senza
    allocare. Le costanti sono quelle reali del riferimento (`ResultStandardExtraction`,
    `OneTextBlockType`, più i tre tipi di `TextBlock` dei builder standard); il `BlockType::CURRENCY`
    citato in `PLAN.md` §4.2 non esiste nel riferimento — il tipo reale è `CURRENCY_STATEMENT`.
  - Un enum d'errore per modulo, tranne `promise_resolution`, che riusa `PromiseError` di
    `core::promise`: le promesse sono un vocabolario condiviso da tre moduli e duplicare l'enum
    avrebbe solo costretto a convertire avanti e indietro.
  - `Promised<T>` implementa `Serialize` ma **non** `Deserialize`: da una stringa non si può
    decidere in generale se sia un `T` o una promessa (per `T = String` sono la stessa cosa). La
    scelta spetta alle entità di `output::classes` (M8), tipo per tipo.
  - `api::core` riesporta anche `BlockType`, `BlockValue`, `BlockValueError` e `PromiseError`,
    che `PLAN.md` §9 non nominava: sono i tipi dei campi pubblici di `PdfBlock`/`TextBlock` e gli
    errori dei loro accessori — senza, i due blocchi non sono utilizzabili da fuori.
- 2026-08-23 — M3 chiusa (TDD: test-writer poi implementer, come da `PLAN.md` §14; review
  adversariale con `critic` prima della chiusura). Decisioni di design confermate dall'utente
  durante l'implementazione (dettagli e motivazione in `PLAN.md` §13):
  - `select::pdf_line::text` usa matching verbatim ad ancore (niente `onig`); un vecchio
    doc-comment di stub che parlava di "regex" era impreciso, non una richiesta di design.
  - `CellGeometry` è un solo tipo canonico, in `tabularizer::coordinates` (quello validato,
    usato davvero dall'algoritmo) — il riferimento ne aveva due solo per via del confine PyO3,
    che qui non esiste.
  - `pdf_extract::relative` (`OptionallyRelative<V,R>`/`RelativeInfo<V>`) resta agganciato a
    `&[PdfLine]`, non genericizzato su un tipo di contesto: è solo spostato di un livello da
    `select::relative`, non ridisegnato.
  - `PdfLine.font: Font` resta normalizzato a costruzione (come il riferimento, per
    performance), ma `Font` come dato vive in `pdf_line.rs`; gli `impl Container`/`Overlappable`/
    `AtomOperations` che lo rendono selezionabile, più `FontSet`, restano in
    `select::pdf_line::font` — la posizione di un `impl` non è vincolata a quella del tipo.
    `PdfLine::area()` è invece un accessore derivato da `bbox: Rectangle` al bisogno, non un
    campo cache: costruire un `Area` da un `Rectangle` non fa lavoro reale da risparmiare, quindi
    niente campo ridondante da tenere sincronizzato.
  - `pdf_extract::position` e `pdf_extract::tabularizer::collapse` avevano una dipendenza
    circolare fra moduli (`position` importava `SplittingState`/`NullableState` da `collapse`,
    `collapse` importava `ColumnConfig`/`TableConfig` da `position`) — compilava comunque (Rust
    non vieta i cicli fra moduli dello stesso crate) ma contraddiceva la disciplina di layering
    seguita ovunque altrove in M3. Risolta spostando le definizioni di `SplittingDirection`/
    `SplittingState`/`NullableState` in `position.rs`; `tabularizer::collapse` le ri-esporta dal
    percorso storico (`pub use`) così i test esistenti (che le importano da lì) restano invariati.
  - `commons.rs` (non verbatim, §0: il riferimento è confine PyO3 al 100%) ha un'API pensata da
    zero — `CommonsError` (`thiserror`), `select_expected_text`/
    `extract_text_pdf_block_or_fail_page` operanti su un `PdfLineSet` concreto (non
    `PdfLineSelection`, per non dipendere da `select::relative`) — che sarà il contratto su cui
    M5 costruirà il motore.
  - `collapse_table_rows` panica (`.unwrap()`) se `indexes` è vuoto e non c'è una `TableConfig`
    esplicita per le colonne: comportamento ereditato **verbatim** dal riferimento, non una
    regressione del porting. Stesso trattamento di `Set::Universe / _` (M1): limite noto e
    accettato, non un target per M3 — da rivedere quando M5 collega questa funzione a pagine
    reali, dove "zero celle estratte" è un input plausibile, non un errore di programmazione.
  - Un warning di `clippy` (`useless_conversion`) resta in un test di
    `select/pdf_line/area.rs`: dipende dal fatto che `Rectangle::SubtractSubsetRes` è un tipo
    identità, esattamente come nel riferimento (a differenza di `SubtractOverlappingRes`, che usa
    un enum dedicato) — il test replica lo stesso idioma `.into()` del test originale. Non
    correggibile senza toccare i test (vietato) o divergere dalla forma `AtomOperations`
    imposta dal porting verbatim: lasciato così, documentato.

- 2026-08-23 — M4 avviata solo in parte (TDD: test-writer poi implementer, come da `PLAN.md` §14).
  **Domanda sulla portata di M4, sollevata durante la pianificazione** (dettagli in
  `agent-memory/M4-implementation-plan.md` §0): ~10 dei ~15 pipe di `standard_funcs` elencati nel
  riferimento dipendono da tipi che in questo crate non esistono ancora — `FilterData`/`Extracted`
  (previsti per M5, il motore) oppure le entità di `output::classes` come `Fund`/`Equity`/`Bond`/
  `ManagementCompany` (previste per M8). Tre opzioni proposte: A) spostare esplicitamente quelle 10
  funzioni a M5/M8; B) anticipare ora stub minimi dei tipi mancanti per chiudere M4 "alla lettera";
  C) lasciare M4 aperta/parziale finché M5 non esiste, poi finire i pipe dipendenti in una passata
  successiva. **L'utente ha scelto l'opzione C.** Di conseguenza M4 resta 🟡 in corso, non ✅: il
  lavoro fatto in questa sessione è nella tabella sopra; le ~10 funzioni dipendenti
  (`TextFilterSfdrArticleStandard`, `TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`,
  `TextFilterInvestmentsStandard`, `DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
  `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
  `DeserializerInvestmentsManagerStandard`, `DeserializerInvestmentStandard`,
  `DeserializerAssetsStandard`) sono assenti dal codice — niente stub, niente `todo!()` — e restano
  deferite a dopo M5 (e, per quelle che dipendono da `output::classes`, a dopo M8). I doc-comment
  di modulo (`formats_utils/deserialize.rs`, `formats_utils/text_filter.rs`) lo dicono già in-code.
  Decisioni di design prese/confermate durante l'implementazione della parte autosufficiente:
  - `extract_currency_from_text` usa `onig` per i suoi due pattern fissi (`\b[A-Z]{3}\b` per i
    codici ISO, `\b{nome membro}\b` per i nomi di valuta), anche se sono pattern fissi di libreria
    e non autore-scritti come quelli di `matcher.rs`: il piano proponeva Rust puro (niente `onig`
    in questo file). L'utente ha scelto `onig` esplicitamente ("perché è più veloce") — è una
    scelta deliberata, non una svista rispetto al piano.
  - I warning `tracing::warn!` di forced-cast in `to_float`/`to_int`/`perc_to_float` (`cast.rs`)
    sono già emessi in M4, anche se lo span con `page`/`company`/`field` che popolerebbe quelle
    colonne di `.log.csv` non si apre prima di M5 — stesso trattamento già accettato per "Matched
    Company" sempre vuota (M0). L'utente ha confermato: "Log now (recommended)".
  - **Bug scoperto durante il porting, corretto su istruzione esplicita dell'utente** (per la
    convenzione di questo workspace un fix di comportamento ereditato da `freeports_core`/Python va
    chiesto, non deciso unilateralmente — fatto correttamente qui). Il riferimento (Python e la
    porzione già portata in Rust di `freeports_core`) elimina silenziosamente il segno dei numeri
    negativi in `to_float`/`to_int`/`perc_to_float` (es. `to_float("-3.5")` restituiva `Ok(3.5)`).
    Su istruzione dell'utente le tre funzioni hanno ricevuto un nuovo parametro finale
    `keep_sign: bool`: un `-` conta come segno genuino solo se è il primo carattere non-spazio
    dell'input trimmato (subito prima del contenuto numerico, es. `"-3.5"`, `"- 3.5"`); un `-`
    altrove (es. `"3.0 -"`, `"$100-"`) resta rumore senza contributo di segno **indipendentemente
    da `keep_sign`**. `keep_sign: true` con un `-` genuino nega il risultato; `keep_sign: false`
    riproduce esattamente il comportamento del riferimento (sempre non-negativo) — è il valore che
    i call site futuri dovrebbero passare per compatibilità con la pipeline esistente, salvo campi
    noti per aver bisogno di negativi. `"-"` da solo (senza cifre) resta un errore in entrambi i
    casi. Divergenza voluta dal comportamento di `freeports_core`, su istruzione dell'utente — non
    un'iniziativa del porting.
  - Una review di `critic` ha trovato che il doc-comment di `matcher.rs` sovrastimava un limite
    (sosteneva che il pattern con gli ancoraggi tolti "non è davvero ancorato"), limite che in
    pratica non si manifesta perché `onig::Regex::is_match` forza il match dell'intera stringa
    dalla posizione 0 (verificato nel sorgente del crate `onig`). Corretto il doc-comment e aggiunto
    un test end-to-end (attraverso `match_company`, non solo sulla stringa del pattern) che pinna il
    comportamento reale di match/no-match sull'ancoraggio. È una correzione di documentazione/
    copertura test, non un cambio di comportamento — la logica di rimozione dei caratteri di
    ancoraggio resta intatta, verbatim dal riferimento.

  Stato finale verificato: `cargo test` — 1161 passati, 0 falliti, 0 ignorati; `cargo clippy
  --all-targets` — solo il warning preesistente e documentato in `select/pdf_line/area.rs` (M3),
  nessun warning nuovo; `grep -rn "todo!()" src/` — nessun risultato in tutto il crate.

- 2026-08-23 — M5 chiusa, e subito dopo M4 integrata con tutto ciò che M5 rendeva possibile (una
  sola sessione, su richiesta esplicita dell'utente: *"la chiusura di M4 dipenda solo da
  `output::classes`"*). Piano di lavoro dettagliato in `agent-memory/M5-implementation-plan.md`.
  **Decisioni prese dall'utente all'apertura di M5:**
  - **`FilterData` — risposta alla domanda che bloccava M5** (`PLAN.md` §13 punto 1). Si tiene la
    semantica del riferimento: enum a due varianti mutuamente esclusive, primo step dello schedule
    ⇒ solo le target companies, step successivi ⇒ solo l'accumulo dei risultati di **tutti** gli
    step precedenti. Conseguenza accettata: un pipe che ha bisogno delle target companies
    (oggi solo `TextFilterInvestmentsStandard`) funziona unicamente se schedulato al primo step —
    è già così nel riferimento, ed è pinnato da un test.
  - **`Page::raw` aggiunto subito**, benché il primo consumatore arrivi con M7 (i pipe Python).
    Motivazione dell'utente: saperlo da ora impedisce di costruire codice che dipende da
    `Clone`/`PartialEq` su `Page`, derive che quel campo rende comunque impossibili (`Py<T>` è
    `Clone` solo con la feature `py-clone`, non abilitata). Risolve anche la contraddizione fra
    `PLAN.md` §4.4 (che lo prevede in `core::page`) e §2 principio 1 (PyO3 solo ai confini): il
    campo è privato, e l'unico test che lo tocca è isolato in un `mod python_boundary`.
  - **Pagina la cui page class compare in due step dello schedule: si accumulano i risultati**,
    non si sovrascrivono. È una **divergenza voluta** dal riferimento, dove
    `res[(doc, page)] = risultati_dello_step` fa sparire dall'output i risultati del primo step
    (che però hanno alimentato il `filter_data` del secondo). Nei casi normali — page class in un
    solo step — il comportamento è identico; da ricordare nel confronto output di M10.

  **Decisioni forzate dallo stato del crate** (annotate, non chieste: non c'era alternativa):
  - `Algorithm::load` non è M5 ma M7: legge il repo formati, che è tutto stub. M5 fornisce
    `Algorithm::new` con le tre validazioni che il riferimento fa in `Algorithm.__new__`
    (pipeline di classificazione note; page class di schedule e mapping coincidenti; nessuna
    pipeline non mappata né mappata e mai usata).
  - `Extracted` nasce **parziale**: solo `Promises` e `PageClass`. Le dieci varianti d'entità di
    `PLAN.md` §5.4 vivono in `output::classes` (M8) e anticiparle avrebbe rifatto due volte il
    lavoro di M8 — oltre a contraddire la richiesta dell'utente di lasciare `output::classes`
    come unica dipendenza aperta di M4.
  - `Algorithm::apply`/`apply_multidocument` restituiscono `DocumentOutcome`/`PageOutcome`, la
    forma tipizzata del dict `{(doc_name, page_n): [...]}` del riferimento: `DocumentResults` è
    `output::routines` (M8), che lo convertirà.
  - `PageClassFinalizer::Python` di `PLAN.md` §5.5 diventa `PageClassFinalizer::Custom(Arc<dyn
    PageClassFinalize>)`: M7 ci innesta il `compute_page_class` dell'autore senza che `core`
    conosca PyO3.
  - Modulo nuovo non previsto da `PLAN.md`: `core::pipeline::data`, che ospita
    `PipeError`/`FilterData`/`Extracted`/`PromiseEntries` — il vocabolario condiviso da tutti e
    cinque i pezzi del motore. `core::pipeline` lo ri-esporta, quindi il percorso pubblico è
    comunque `core::pipeline::{...}`.

  **Altre decisioni prese durante l'implementazione:**
  - `Segment<P>` deduplica per identità (`Arc::ptr_eq`), come il `set` di oggetti senza
    `__hash__` del riferimento; `PipelinesBundle` deduplica invece per **nome**, perché le
    pipeline arrivano sempre da una mappa `nome → Pipeline` e là "stessa pipeline" e "stesso
    nome" sono la stessa cosa.
  - `ScheduleStep` conserva l'ordine di inserimento invece dell'ordine di hash di un `set`
    Python (coerente con D5, che già lo stabiliva per i pipe): l'ordine di elaborazione dentro
    uno step è deterministico.
  - `PipelinesBundle::apply_deserialize` coincide con `apply`. Nel riferimento le due differiscono
    solo per il filtraggio dei `None` restituiti dai pipe; qui quei `None` non esistono, perché un
    pipe che non ha nulla da dire restituisce un vettore vuoto e "pagina non classificata" è la
    variante esplicita `Extracted::PageClass(None)`. Il metodo resta perché è una delle tre API
    parziali che `freeports-dev` usa (`PLAN.md` §5.3).
  - `Algorithm::apply_deserializer` segue la firma di `PLAN.md` §5.5 (parte da blocchi di testo
    già pronti) e non quella del riferimento (che riparte dalla pagina): così i tre metodi per
    segmento decompongono davvero la catena in tre pezzi concatenabili.
  - `PipeError::from_commons` mantiene la promessa scritta in M3 nel doc-comment di
    `pdf_extract::commons`: `CommonsError::PageParseFail` diventa il fallimento **non fatale** di
    pagina, `ExpectedTextNotFound` un `PipeError::Extraction`. Non è un `impl From` perché il nome
    del pipe non è ricavabile dall'errore.
  - Asimmetria del riferimento conservata: un fallimento di pagina è assorbito **solo** nel ciclo
    dello schedule, non durante la classificazione delle pagine.
  - `ScheduledPage` porta un `doc_index` oltre al `&Document`: due documenti possono
    legittimamente avere lo stesso id, quindi ricomporre i risultati per id sarebbe sbagliato.
  - `core::classes::BlockType` ha ricevuto due costanti nuove, `EQUITY_TARGET` e `BOND_TARGET`
    (`ResultStandardFiltering` del riferimento): le produce `TextFilterInvestmentsStandard`.
    Modifica puramente additiva a codice M2, stesso precedente dei derive aggiunti a M1 durante M2.
  - `PdfBlocksTable` (porting di M4/M5) tiene **solo indici** nella lista piatta invece delle due
    viste aliasate del riferimento: uscendo da Python l'aliasing non esiste e non serve, e sparisce
    la possibilità che le due viste divergano. Dove il riferimento andrebbe in `IndexError` su una
    tabella incoerente, qui c'è `StandardFuncsError::InconsistentTable` — il modulo non è fra
    quelli da portare *verbatim* (`PLAN.md` §0), quindi tipizzare l'errore è lecito.
  - Quirk del riferimento conservati **di proposito** in `TextFilterInvestmentsStandard`, ciascuno
    con un test che lo pinna: il controllo "le posizioni devono essere diverse" scatta solo se
    entrambe le posizioni opzionali sono presenti; se il ciclo sulla tabella non produce righe, il
    risultato è vuoto e viene buttato anche il blocco del nome del fondo già costruito; la coda del
    ciclo riusa la colonna lasciata dall'ultima iterazione, non quella dell'ultimo blocco.
  - I pattern `PERC_REGEXES`/`DATE_REGEXES` sono ancorati con `\A`: `re.match` di Python tenta il
    match solo dalla posizione 0, mentre `onig::Regex::captures` cerca ovunque. Senza ancoraggio si
    inventerebbe un `interest rate` su contenuti che iniziano con una cifra — regressione reale già
    documentata nel riferimento, qui pinnata da un test dedicato.

  Stato finale verificato: `cargo test` — 1365 test unitari + 10 d'integrazione passati, 0 falliti,
  0 ignorati; `cargo clippy --all-targets` — solo il warning preesistente e documentato in
  `select/pdf_line/area.rs` (M3); `grep -rn "todo!()" src/ tests/` — nessun risultato.

- 2026-08-23 — M6 chiusa (TDD: `implementation-planner` → `test-writer` → `implementer` → review
  adversariale di `critic` prima della chiusura, come da `PLAN.md` §14; piano dettagliato in
  `agent-memory/M6-implementation-plan.md`). Porta `pdf_blks_acquire.py` in `input::document`,
  **non** uno dei moduli verbatim di `PLAN.md` §0 (libero di essere ridisegnato, a patto di
  preservarne la logica) — diviso in `page_dict` (struct tipizzata del "page dict" PyMuPDF +
  funzioni pure: `collapse_spans_from_line`, `rotate_bbox`, `rotate_line`, `pdflines_from_pagedict`,
  `pdfimages_from_pagedict`) e `selection` (`pdfline_selection_from_dict`/`_from_str`, che
  costruiscono una `PdfLineSelection`, M3, da configurazione esterna — non hanno a che fare con
  PyMuPDF, ma vivono qui perché è lo stesso modulo Python di origine). Un solo test tocca davvero
  PyMuPDF (`document.rs::tests::python_boundary`, `PLAN.md` §11/§10 D13); tutto il resto — comprese
  le 25 combinazioni geometriche di rotazione/collasso span e le 24 di selezione da dict/stringa —
  è pure Rust su struct costruite a mano.

  **Due domande aperte poste all'utente durante la pianificazione, entrambe risolte con l'opzione
  raccomandata:**
  - **Q1** — una riga PDF senza span in `collapse_spans_from_line`: il riferimento va in
    `ZeroDivisionError`; un porting letterale della sola aritmetica produrrebbe `NaN` in silenzio
    (peggio del riferimento). **Confermato**: restituisce `Vec::new()` — nessun panic, nessun
    `NaN`, trattata come riga vuota.
  - **Q2** — `load_document`/`load_document_pages` (apri un PDF reale con `fitz`, itera le pagine,
    costruisci un `Document`/`Vec<Page>`): non elencate da `PLAN.md` §9, ma senza di esse nessun
    consumatore esterno potrebbe mai costruire un `Document` da un path reale. **Confermato**:
    implementate in M6, riesportate sotto un nuovo `api::input` non previsto da §9 — stesso
    trattamento già riservato a `TablePosMeasureUnit` (buco fra §9 e lo scope necessario,
    documentato in `api.rs` e qui, non un'omissione silenziosa).

  **Terza domanda emersa durante la review di `critic` prima della chiusura, anch'essa risolta con
  l'opzione raccomandata:** `pdflines_from_pagedict` costruiva ogni `PdfLine` via
  `PdfLine::new`/`Rectangle::new` (M3, verbatim), che panicano su `font_size <= 0.0` o bbox
  invertita (`x0 > x1` o `y0 > y1`). Fino a M6 quel costruttore era raggiungibile solo da dati
  costruiti a mano (test, autore di formato); `load_document` lo rende per la prima volta
  raggiungibile da un vero `page.get_text("dict")` PyMuPDF, cioè da input non fidato — un PDF reale
  con uno span a corpo non positivo o bbox invertita avrebbe fatto panicare l'intero processo
  invece di poter saltare la singola pagina/riga. **Confermato**: aggiunta una guardia in
  `pdflines_from_pagedict` che scarta silenziosamente lo span incriminato (stesso trattamento della
  riga senza span, Q1) con un `tracing::warn!` per non perdere visibilità (stesso stile di
  `cast.rs`, M4), invece di lasciare che l'aritmetica raggiunga il costruttore panicante. Non è un
  bug ereditato da `freeports_core`/Python (quel confronto qui non si applica: `PdfLine::new` è
  codice di `freeports_lib`, non del riferimento Python di questa milestone) — è un limite
  preesistente di M3 mai stato raggiungibile da input non fidato prima d'ora, e la correzione
  riguarda solo la nuova guardia introdotta qui in M6.

  **Altre decisioni prese durante l'implementazione (non richiedevano conferma, `agent-memory/
  M6-implementation-plan.md` §1):**
  - Il "page dict" PyMuPDF diventa uno struct Rust tipizzato (`PageDict`/`PageDictBlock`/
    `PageDictLine`/`PageDictSpan`), non un `HashMap`/`serde_json::Value` dinamico: è l'unico modo
    di avere "funzioni pure su dict costruiti a mano" (`PLAN.md` §11) senza che quelle funzioni
    debbano gestire forme malformate — tutta la fallibilità di forma si concentra nell'unico punto
    che estrae `PageDict` da un `Bound<PyDict>` reale (`PageDict::from_py`, nessun test dedicato,
    esercitato solo transitivamente dal test di confine).
  - `PageError` (già in `core::page`, M5) riusato per gli errori di forma del page dict, invece di
    un nuovo enum: `ParseFail`/`LineParseFail` modellano già esattamente "pagina non
    interpretabile"/"riga non interpretabile". Un nuovo `DocumentError` copre invece i fallimenti a
    livello di documento (apertura del PDF, o una pagina specifica con il suo numero).
  - `pdfline_selection_from_dict`/`_from_str` entrambe fallibili con lo stesso
    `LineSelectionError`, e `_from_str` **delega** a `_from_dict` dopo aver parsato la grammatica
    compatta con `onig` (precedente diretto: `extract_currency_from_text`, M4, scelta esplicita
    dell'utente "perché è più veloce") — una sola implementazione della combinatoria dei criteri,
    non due. `rotate_lines_inplace` diventa `rotate_line`, non mutante (non verbatim, D-M6-1 del
    piano: la mutazione in-place è un dettaglio implementativo Python, non logica osservabile).
  - Grammatica `onig`: i cinque gruppi catturanti (font, font_size, y_range, area, text) sono
    catturati per **posizione**, non per nome (l'API Rust di `onig` non ha un equivalente di
    `groupdict()`), rendendo non-catturanti tutti i gruppi interni ausiliari — verificato da
    `critic` prima della chiusura: nessun rischio di disallineamento posizionale, perché il
    pattern Rust ha esattamente 5 gruppi catturanti mappati 1:1, a differenza del pattern Python
    (22 gruppi) da cui è stato ridisegnato. Confermata anche la sottigliezza grammaticale che
    l'alternativa "area piena" richiede una coppia di parentesi *ulteriore* che avvolge le due
    coppie di range (`((x0:x1)(y0:y1))`, non `(x0:x1)(y0:y1)` nudo) — verificata riproducendo il
    pattern Python `LINE_SET_REGEXP` fuori dal crate.

  Stato finale verificato: `cargo test` — 1417 test unitari + 10 d'integrazione passati, 0 falliti,
  0 ignorati; `cargo clippy --all-targets` — solo il warning preesistente e documentato in
  `select/pdf_line/area.rs` (M3), nessun warning nuovo; `grep -rn "todo!()" src/ tests/` — nessun
  risultato.

## Voci aperte trasversali alle milestone (non bloccano nessuna milestone corrente)

- **`TablePosMeasureUnit`** (`PLAN.md` §9) — nessun riferimento esiste da nessuna parte; gap fra
  §9 e lo scope reale di M3, lasciato non implementato e non riesportato da `api`. Da chiarire
  prima che una milestone futura ne dipenda davvero.
- **`pub` vs `pub(crate)`** sull'intero albero interno (`commons`, `core`, `formats_utils`, ...) —
  da M0 tutto è `pub mod`, non `pub(crate) mod` come richiede `PLAN.md` §14; i tipi interni sono
  raggiungibili da fuori crate bypassando `api`. Non introdotto da M3 (che continua il pattern
  esistente), ma trasversale a tutte le milestone finora. Confermato dall'utente (2026-08-23):
  si lascia com'è, si affronta come task a parte (candidato: pulizia M10), non dentro una singola
  milestone.
- **`formats_utils::pdf_extract::standard_funcs` non è assegnato a nessuna milestone** (emerso
  durante M5). `PLAN.md` §9 elenca `PdfExtractPageClassifyStandard`,
  `PdfExtractInvestmentsStandard`, `PdfExtractCurrencyStandard`, `PdfExtractCurrencyConstant`,
  `PdfExtractFundStandard`, `PdfExtractManagmentCompanyStandard`, `PdfExtractSfdrArticleStandard`,
  `PdfExtractAssetsStandard` fra la superficie pubblica, ma §11 non li mette in nessuna riga: M3
  copre `pdf_line`/`relative`/`select`/`position`/`tabularizer`/`commons`, M4 le due
  `standard_funcs` di `text_filter` e `deserialize`. Il file è ancora uno stub a tre righe. Sono i
  pipe che `formats_repo::structured` (M7) costruisce dai CSV, quindi il posto naturale è M7 o una
  passata dedicata — ma **va deciso, non assunto**. Stessa categoria di `TablePosMeasureUnit`: un
  buco fra §9 e §11, non un'omissione silenziosa.
- **`api::input` (`load_document`/`load_document_pages`) non è elencato da `PLAN.md` §9** (che per
  `input` nomina solo `load_target_companies`/`compile_target_companies`, `input::companies_db`) —
  aggiunto comunque in M6 perché senza un punto d'ingresso che apra un PDF reale nessun consumatore
  esterno potrebbe mai costruire un `Document`. Confermato dall'utente (2026-08-23, M6 Q2): stesso
  trattamento di `TablePosMeasureUnit`, documentato in `api.rs`, non bloccante.

## Domande aperte

Vedi `PLAN.md` §13. Le domande di M1 (`int_value()`, panic di `Set::Universe / _`), quella
sollevata da M2 (riferimenti promise pendenti) e le cinque sollevate durante M3 (vedi sopra) sono
state confermate dall'utente. La domanda sulla portata di M4 (opzioni A/B/C per le ~10 funzioni di
`standard_funcs` dipendenti da M5/M8, `agent-memory/M4-implementation-plan.md` §0) è stata risolta
il 2026-08-23: l'utente ha scelto l'opzione C (vedi voce sopra) — non è più una domanda aperta, è
lavoro deferito. Anche le tre domande di M5 (semantica di `FilterData`, `Page::raw`, pagina in due
step dello schedule) sono state risolte dall'utente il 2026-08-23, nella voce sopra: **`FilterData`
non blocca più nulla**. Le due domande di M6 (riga senza span in `collapse_spans_from_line`,
`load_document`/`load_document_pages` fuori da §9) e la terza emersa dalla review di `critic` prima
della chiusura di M6 (guardia contro il panic di `PdfLine::new`/`Rectangle::new` su input non
fidato) sono state risolte dall'utente il 2026-08-23, nella voce di chiusura M6 sopra.

Restano aperte solo le domande pre-esistenti non ancora toccate: rigenerazione dei fixture
`freeports-dev` (M8 o M10), campo dedicato per la colonna `Matched Company` del `.log.csv` (non
blocca nulla), più le tre voci trasversali elencate sopra (`TablePosMeasureUnit`,
`pub`/`pub(crate)`, e la milestone di `pdf_extract::standard_funcs`).

**Lavoro deferito, non una domanda aperta**: le 10 funzioni di `standard_funcs` che costruiscono
entità di `output::classes` (`DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
`DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
`DeserializerInvestmentsManagerStandard`, `DeserializerInvestmentStandard`,
`DeserializerAssetsStandard`, `TextFilterSfdrArticleStandard`,
`TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`) restano da implementare non
appena M8 esiste. `TextFilterInvestmentsStandard`, che era l'unica bloccata dal solo motore, è
stata scritta alla chiusura di M5: **`output::classes` è ora l'unica dipendenza che tiene aperta
M4**, come chiesto dall'utente.
