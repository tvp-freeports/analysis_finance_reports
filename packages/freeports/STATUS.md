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
| M4 | `text_filter` + `deserialize` | ✅ chiusa | Chiusa da M8: le 8 funzioni deferite (`DeserializeSfdrArticleStandard`, `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`, `DeserializerInvestmentsManagerStandard`, `DeserializerAssetsStandard`, `TextFilterSfdrArticleStandard`, `TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`) sono ora implementate — vedi la voce di chiusura M8 sotto per dettagli |
| M5 | Motore (pipeline/algorithm) | ✅ chiusa | `core::page`, `core::schedule`, `core::pipeline::{data,segment,bundle}`, `core::pipeline::Pipeline`, `core::algorithm`; 1365 test unitari + 10 d'integrazione (`tests/algorithm_end_to_end.rs`) verdi, `cargo clippy --all-targets` senza warning nuovi; `api::core` estesa con `Pipeline`/`Algorithm` e il resto del motore. `Algorithm::load` resta a M7 (legge il repo formati) |
| M6 | `input::document` | ✅ chiusa | `input::document::{page_dict, selection}` (+ modulo radice); 1417 test unitari + 10 d'integrazione verdi, `cargo clippy --all-targets` senza warning nuovi; `api::utils::pdf_extract` estesa con le 4 funzioni di §9 rimaste da M3, nuovo `api::input` |
| M7 | `formats_repo` | ✅ chiusa | `id_format`, `metadata`, `orchestration`, `structured::{tables,page_classify,investments}`, `semistructured::{formats_mapping,args,native}`, `unstructured::{loader,py_pipe}`, `Algorithm::load`; piu' `formats_utils::pdf_extract::standard_funcs` (8 pipe, D-M7-1) e `output::classes::{fund,investment}` anticipati da M8 (D-M7-2). 1828 test unitari + 32 d'integrazione verdi, `cargo clippy --all-targets` senza warning nuovi; `api` estesa con `standard_funcs`, `formats_repo`, `output` e `get_table_coordinates`/`TablePosMeasureUnit` |
| M8 | `output` | ✅ chiusa | `output::classes::{fund_change_name, fund_esg_indicator, fund_sfdr_classification, fund_assets, assets_manager}`, le 7 varianti restanti di `core::pipeline::Extracted`, le 8 funzioni deferite di M4, `output::files_schema`, `output::routines::{accumulate, write}`. I 3 bug di fixture nei test scritti da `test-writer` (vedi la nota di chiusura sotto) sono stati corretti; `cargo test`: 2092 passati/0 falliti (lib) + 34 d'integrazione (10+22+2) passati; `cargo clippy --all-targets`: solo il warning preesistente di `select/pdf_line/area.rs` (M3) |
| M9 | `cli` | ✅ chiusa | `input::{companies_db, download}` (stub pre-esistenti, pull-in per Q1), `cli::{conf_parse::DocumentSpec, partial_config, config_locations::{cmd,env,file}, freeports_config, batch, job, output, run::execute}`, `main.rs` reale; `core::tracing_setup::Verbosity` riaperto (M0, Q5: 6 livelli, `-v`/`-q` manopole indipendenti) e `output::routines::write` riaperto (M8, Q6: `OutFlags::separate_out` + profili `SingleFile`/`Structured`/`compressed` reali); 2428 test unitari + 63 d'integrazione (10+29+22+2) verdi, `cargo clippy --all-targets` pulito salvo il warning preesistente M3 e 5 nuovi confinati a codice di test; `api::cli` abilitata, chiude `PLAN.md` §9 |
| M10 | Chiusura e confronto | 🟡 in corso | API Python (`src/python/`, `#[pymodule] freeports` — niente `_native`/`_internals`), `pyproject.toml`/`cdylib`, `freeports_dev` migrato ai percorsi pubblici. **Nessuna modifica al repo formati.** 2471 test unitari + 63 d'integrazione verdi, `cargo clippy --all-targets` come a M9 (6 warning, tutti preesistenti). `pytest tests/formats` sul repo formati reale: 248 passati / 11 falliti contro la baseline 252/7 del motore Python — **tutti i CSV di business coincidono su tutti e 21 i formati**; restano 7 fallimenti su `.log.csv` (gli stessi 7 formati che fallivano anche in baseline) e 4 fixture a pagina singola diventate obsolete. Vedi la voce M10 sotto |

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

- 2026-08-23 — M7 chiusa (TDD modulo per modulo: contratto nel doc-comment, test, implementazione;
  piano dettagliato in `agent-memory/M7-implementation-plan.md`). Porta l'intero `formats_repo` —
  `id_format`, `metadata`, `orchestration`, i tre livelli `structured`/`semistructured`/
  `unstructured` e la loro fusione in `Algorithm::load` — più i due moduli che ne erano
  prerequisiti.

  **Tre domande poste all'utente prima di scrivere codice** (`PLAN.md` §13 regola generale; il
  punto 6 diceva espressamente "da decidere prima di M7"), tutte risolte:

  - **D-M7-1 — `formats_utils::pdf_extract::standard_funcs` appartiene a M7**, con **tutti e otto**
    i pipe (`PdfExtractPageClassifyStandard`, `PdfExtractInvestmentsStandard`,
    `PdfExtractFundStandard`, `PdfExtractCurrencyStandard`, `PdfExtractCurrencyConstant`,
    `PdfExtractSfdrArticleStandard`, `PdfExtractManagmentCompanyStandard`,
    `PdfExtractAssetsStandard`). Chiude `PLAN.md` §13 punto 6.
  - **D-M7-2 — si anticipano da M8 `output::classes::{fund, investment}`**, per poter costruire
    `DeserializerFundStandard`/`DeserializerInvestmentStandard` e quindi il segmento `deserialize`
    della pipeline structured `investments`. **M7 chiude completa (✅), non 🟡.** È una divergenza
    consapevole dalla scelta fatta per M4 (opzione C, deferire): là il costo era solo rimandare,
    qui sarebbe stato lasciare non verificabile end-to-end proprio la fusione dei tre livelli, che
    è il focus di test dichiarato di M7.
  - **D-M7-3 — `unstructured` si implementa ora, duck-typed, con moduli Python sintetici.** I
    moduli d'autore di un repo formati reale importano `freeports.core.Pipeline`,
    `freeports.standard_funcs.*` ecc., cioè l'API Python che `PLAN.md` §0 esclude da questa fase:
    **un repo formati reale non è caricabile finché i binding non esistono**, ed è un limite
    dichiarato del perimetro, non un difetto del porting. Il confine accetta perciò i valori per
    *forma* — `pipelines` come oggetto/mappa con `pdf_extract`/`text_filter`/`deserialize`; i
    blocchi con `type_block`/`metadata`/`content` (più `pdf_block`); le promesse con
    `id`/`strict`/`multiple` — che è esattamente ciò che le classi di `freeports_core` già
    espongono, così che l'arrivo dei binding non richieda una seconda passata.

  **Una terza domanda aperta di `PLAN.md` §13 risolta strada facendo, con prove e non per
  decisione**: il punto 4, `TablePosMeasureUnit`. Il tipo *esiste* nel riferimento — in
  `formats/utils/pdf_extract/position.py`, non in Rust — ed è l'unità di misura della `tolerance`
  del `get_table_coordinates` che parte dalle **righe** di una pagina invece che da celle già
  costruite. M3 non ne aveva trovato traccia perché quel wrapper non era ancora stato portato. È
  lui, e non la funzione per celle, quello che §9 elenca (accanto proprio a `TablePosMeasureUnit`)
  e quello che gli autori di formato chiamano davvero. M7 lo porta in `pdf_extract::tabularizer`
  come `get_table_coordinates_from_lines`, e **`api::utils::pdf_extract` lo esporta sotto il nome
  `get_table_coordinates`**, rinominando `get_table_coordinates_from_cells` quello esportato da M3.
  È l'unica rinomina della superficie pubblica finora, e riguarda solo l'alias di `api`: dentro il
  crate i due nomi restano quelli di sempre.

  **Decisioni di design prese durante l'implementazione** (non richiedevano conferma):

  - `id_format::computed_ids` porta anche la parte **di gruppo** di `add_pipe_index` (il
    `groupby().cumcount()` di pandas), che il porting Rust parziale in `freeports_core` non aveva
    perché non esprimibile riga per riga. Le tre politiche di `FkRelation` sono conservate
    verbatim, **quirk compreso**: in `OneToMaybe` il contatore avanza solo fra le righe *senza*
    indice esplicito, quindi un indice dichiarato e uno derivato possono collidere (pinnato da un
    test dedicato).
  - `orchestration::get_schedule`/`get_mapping` ricevono i nomi delle pipeline già costruite come
    **parametro**, invece di richiamare il caricamento del repo come fa il riferimento nel ramo di
    fallback: quella è una dipendenza circolare che nel riferimento è tollerabile solo perché passa
    da Python. Qui il chiamante — `Algorithm::load` — le pipeline le ha già in mano.
  - `impl Algorithm { pub fn load(...) }` è scritto **dentro `formats_repo.rs`**, non in
    `core::algorithm`: un `impl` inerente può stare in qualunque modulo del crate che definisce il
    tipo, quindi la API resta quella di `PLAN.md` §5.5 senza invertire il layering (`formats_repo`
    è costruito sopra `core`, non viceversa). Stessa disciplina con cui M3 ha spezzato il ciclo
    `position`/`tabularizer`.
  - `input::document::selection` ha una funzione nuova, `is_pdfline_selection`, ancorata **anche in
    fondo**: `pdfline_selection_from_str` non rifiuta nulla (ogni gruppo del pattern è opzionale,
    quindi combacia con qualunque stringa), mentre la validazione delle tabelle CSV deve poter dire
    "questa cella non è una selezione" — è il `str.match(f"^{LINE_SET_REGEXP_PATTERN}$")` di pandera
    del riferimento.
  - `TablePosAlgorithm::from_expression` (additivo a codice M3) è il `TablePosAlgorithm.from_dict`
    del riferimento, e delega a `commons::flag_expr` (M1), che accetta il nome singolo del
    riferimento e in più le espressioni booleane: è un superinsieme, quindi non si perde nulla.
  - Le entità di `output::classes` usano `OrderedFloat<f64>` e non `f64` nudo nei campi numerici:
    `core::pipeline::Extracted`, che le trasporta, deriva `Eq`, e un `f64` lo renderebbe
    impossibile. Stessa scelta già fatta da `BlockValue::Float` (M2); costruttori e accessori
    parlano comunque `f64`.
  - `Promised<T>` continua a non implementare `Deserialize` (decisione M2); `output::classes`
    fornisce i due moduli serde `serde_promised`/`serde_optional_promised` che **in lettura
    considerano ogni valore risolto**. È corretto per l'uso reale, perché `PLAN.md` §7 impone che
    le promesse siano risolte *prima* della scrittura: nessun file prodotto dal sistema contiene
    un'entità pendente.
  - Tre derive `Debug` aggiunti a codice di milestone precedenti (`TablePosAlgorithm`,
    `CollapseAlgorithm`) e due accessori a `CompanyMatchInfos` (`name`, `normalized_name`, senza i
    quali il confine Python non può passare le società bersaglio a un pipe d'autore): modifiche
    puramente additive, stesso precedente dei derive aggiunti a M1 durante M2.

  **Divergenze volute dal riferimento, ciascuna con il proprio test:**

  - `get_table_coordinates_from_lines`, ramo `company_col`: il riferimento fa `n_cols = max(*cols)`,
    che solleva `TypeError` con una sola cella e produce comunque un vettore di colonne lungo `max`
    invece di `max + 1`. Qui il massimo è calcolato su un iteratore (nessun caso limite) e il
    vettore ha una colonna per ogni indice osservato, perché `PLAN.md` §2 principio 4 vieta i panici
    sul percorso utente. **Non osservabile dai chiamanti attuali**, che passano tutti
    `collapse: false`: in quel caso la configurazione costruita da quel ramo non viene mai letta —
    caveat del riferimento anch'esso conservato e documentato.
  - `PdfExtractAssetsStandard`: dove il riferimento va in `IndexError` (colonne di lunghezza
    diversa, nome di fondo senza token di valuta) qui c'è un errore tipizzato. Entrambe le versioni
    falliscono: cambia solo che qui l'errore ha un nome.
  - `PyDeserializePipe`: in questa fase un pipe `deserialize` d'autore può restituire **solo
    promesse**, perché le entità di `output::classes` non hanno una forma Python finché i binding
    non esistono. Un risultato di forma diversa è un errore esplicito, non un risultato scartato in
    silenzio.

  Stato finale verificato: `cargo test` — 1828 test unitari + 10 (`tests/algorithm_end_to_end.rs`)
  + 22 (`tests/formats_repo_loading.rs`) passati, 0 falliti, 0 ignorati; `cargo clippy
  --all-targets` — solo il warning preesistente e documentato in `select/pdf_line/area.rs` (M3);
  `grep -rn "todo!()" src/ tests/` — nessun risultato.

  **Nota operativa**: `cargo test` richiede il venv attivo
  (`source analysis_finance_reports/venv/freeports-dev/bin/activate`), altrimenti i test di
  confine Python falliscono con `ModuleNotFoundError: fitz`.

## Voci aperte trasversali alle milestone (non bloccano nessuna milestone corrente)

- ~~**`TablePosMeasureUnit`**~~ — **risolta in M7** (2026-08-23): il tipo esiste nel riferimento,
  in `formats/utils/pdf_extract/position.py`, come unità di misura della tolleranza del
  `get_table_coordinates` che parte dalle righe di una pagina. Portato in
  `pdf_extract::tabularizer` ed esportato da `api`. Vedi la voce di chiusura M7 sopra.
- ~~**`formats_utils::pdf_extract::standard_funcs` non è assegnato a nessuna milestone**~~ —
  **risolta in M7** (2026-08-23, decisione D-M7-1 dell'utente): appartiene a M7, con tutti e otto
  i pipe. Vedi la voce di chiusura M7 sopra.
- **`pub` vs `pub(crate)`** sull'intero albero interno (`commons`, `core`, `formats_utils`, ...) —
  da M0 tutto è `pub mod`, non `pub(crate) mod` come richiede `PLAN.md` §14; i tipi interni sono
  raggiungibili da fuori crate bypassando `api`. Non introdotto da M3 (che continua il pattern
  esistente), ma trasversale a tutte le milestone finora. Confermato dall'utente (2026-08-23):
  si lascia com'è, si affronta come task a parte (candidato: pulizia M10), non dentro una singola
  milestone. **M7 lo ha ampliato**: `formats_repo` è tutto `pub`, quindi la pulizia avrà più
  superficie da coprire.
- **`api::input` (`load_document`/`load_document_pages`) non è elencato da `PLAN.md` §9** (che per
  `input` nomina solo `load_target_companies`/`compile_target_companies`, `input::companies_db`) —
  aggiunto comunque in M6 perché senza un punto d'ingresso che apra un PDF reale nessun consumatore
  esterno potrebbe mai costruire un `Document`. Confermato dall'utente (2026-08-23, M6 Q2),
  documentato in `api.rs`, non bloccante. M7 ha fatto lo stesso con `api::formats_repo` e
  `api::standard_funcs`, per le stesse ragioni e con la stessa annotazione in-code.
- **Un repo formati reale non è caricabile finché non esistono i binding Python** (emerso e
  accettato in M7, decisione D-M7-3). I moduli d'autore importano `freeports.core.Pipeline` &
  co., cioè l'API che `PLAN.md` §0 esclude da questa fase. Non blocca nulla ora — i test usano
  moduli sintetici che non importano `freeports` — ma **blocca M10**, che deve confrontare
  l'output con `freeports_core` su un formato reale: o si anticipa una parte dei binding, o M10
  confronta solo formati puramente structured/semistructured. *Da decidere prima di M10.*
- ~~**`OutStructureMode::{SingleFile, Structured}` e `OutFlags::compressed` restano a M9**~~ —
  **risolta in M9** (2026-08-24, decisione Q6 dell'utente): `output::routines::write` (M8,
  riaperto) implementa ora per davvero tutti e tre i profili più la compressione (`flate2`+`tar`),
  e guadagna `OutFlags::separate_out` (un CSV per `Report` invece di uno unico, sostituisce
  `prefix_out`). Vedi la voce di chiusura M9 sotto.
- **Concorrenza dei job in modalità batch (`N_WORKERS`) resta sequenziale** (M9, Q7 confermata
  dall'utente 2026-08-24, come raccomandato): `cli::job`/`cli::batch` validano e portano
  `n_workers` a valle, ma senza vera parallelizzazione — quella diventa un task a parte
  (candidato: dopo M10, o una milestone dedicata). Stesso trattamento già riservato a
  `SingleFile`/`Structured` prima che M9 li chiudesse.
- ~~**Tre test di M8 scritti da `test-writer` restavano falliti**~~ — **corretti il 2026-08-24**
  (fixture, non codice di produzione): un fixture di `DeserializerAssetsStandard` che rompeva
  l'equazione contabile di `FundAssets` senza volerlo, un `expected` di
  `TextFilterManagmentCompanyStandard` costruito con nomi non maiuscolizzati (mentre
  `Fund::name()` lo è sempre, come nel riferimento), e un test di `TextFilterSfdrArticleStandard`
  il cui commento assumeva una rimozione di prefisso ancorata all'inizio della stringa mentre sia
  il riferimento sia il doc-comment del modulo specificano `str::replace` non ancorato. Analisi e
  correzione complete nella voce di chiusura M8 sopra. `cargo test` è verde senza riserve.

## Domande aperte

Vedi `PLAN.md` §13. Le domande di M1 (`int_value()`, panic di `Set::Universe / _`), quella
sollevata da M2 (riferimenti promise pendenti) e le cinque sollevate durante M3 sono state
confermate dall'utente. La domanda sulla portata di M4 (opzioni A/B/C per le ~10 funzioni di
`standard_funcs` dipendenti da M5/M8) è stata risolta il 2026-08-23 con l'opzione C — non è più una
domanda aperta, è lavoro deferito. Le tre domande di M5 (semantica di `FilterData`, `Page::raw`,
pagina in due step dello schedule), le due di M6 più la terza emersa dalla review di `critic`, e le
tre di M7 (D-M7-1/2/3) sono anch'esse risolte, ciascuna nella propria voce di chiusura sopra. Le
otto domande di M9 (Q1-Q8, incluse le due riaperture di milestone chiuse — Q5 su M0, Q6 su M8) sono
anch'esse tutte risolte, riportate per intero nella voce di chiusura M9 sopra e in `PLAN.md` §13.

Restano aperte solo:

- rigenerazione dei fixture `freeports-dev`, che sono pickle Python (M8 o M10);
- campo dedicato per la colonna `Matched Company` del `.log.csv` (non blocca nulla);
- `pub` vs `pub(crate)` sull'albero interno (candidato: pulizia M10);
- **come M10 confronta l'output su un formato reale**, dato che senza binding Python un repo
  formati reale non è caricabile (vedi la voce trasversale sopra) — è la sola domanda nuova che
  M7 lascia aperta.

**Lavoro deferito, non una domanda aperta**: le **8** funzioni di `standard_funcs` che costruiscono
entità di `output::classes` ancora inesistenti (`DeserializeSfdrArticleStandard`,
`DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
`DeserializerInvestmentsManagerStandard`, `DeserializerAssetsStandard`,
`TextFilterSfdrArticleStandard`, `TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`)
restano da implementare non appena M8 esiste. Erano dieci: `DeserializerFundStandard` e
`DeserializerInvestmentStandard` sono state scritte in M7, insieme alle due entità che servivano
loro (D-M7-2). **`output::classes` resta l'unica dipendenza che tiene aperta M4.**

- 2026-08-24 — M8 chiusa (`agent-memory/M8-implementation-plan.md`), e con essa M4 (le 8 funzioni
  deferite sopra sono ora implementate). Le tre decisioni **Q1.1/Q1.2/Q2** dell'utente (2026-08-23,
  vedi `agent-memory/M8-implementation-plan.md` §0) sono riportate in `PLAN.md` §13: `output::routines`
  possiede `OutStructureMode`/`OutFlags` (non `cli::conf_parse`); solo il profilo `Regular` è
  implementato (`SingleFile`/`Structured`/`OutFlags::compressed` sono `WriteFilesError` tipizzati,
  non un panic, deferiti a M9 — annotato come voce aperta trasversale, vedi sotto); `api::output`
  riesporta i nomi reali (`FundRename`/`FundMerge`, `FundSfdrClassification`, `FundEsgIndicator`),
  non la lettera di `PLAN.md` §9.

  **Tre test scritti da `test-writer` restano falliti — analizzati a fondo, non modificabili senza
  toccare codice di test (vietato), segnalati qui per la revisione di `test-writer`/dell'utente:**

  1. `formats_utils::deserialize::standard_funcs::tests::deserializer_assets::num_converter::
     interprets_amounts_as_floats_when_configured_so` — il fixture cambia solo `tot_assets` (a
     `"1.000,5"` → `1000.5`) lasciando `net_assets`/`liabilities` ai default (`800`/`200`, che
     sommano a `1000`): l'equazione contabile di `FundAssets::build` (`liabilities + net_assets ==
     tot_assets`, tolleranza `1e-4`, dallo stesso identico vincolo verificato dal riferimento
     Python/`freeports_core` — non un'invenzione di questo porting) rifiuta lo sbilanciamento di
     `0.5`. Il test si aspetta `.unwrap()` con successo: sembra un fixture che verifica solo la
     conversione numerica senza accorgersi che rompe l'invariante contabile verificato altrove
     nello stesso file (es. `rejects_a_difference_clearly_outside_the_tolerance` in
     `output/classes/fund_assets.rs`).
  2. `formats_utils::text_filter::standard_funcs::tests::text_filter_managment_company::
     builds_the_same_block_as_the_standard_txt_blk_helper` — confronta l'output reale (che usa
     `Fund::name()`, cioè il nome normalizzato e **maiuscolizzato** — `"ALPHA FUND"`/`"BETA FUND"`,
     esattamente come fa anche il riferimento Python in `get_funds`/`item.getattr("name")`) con un
     `expected` costruito da `MatchFund::new("Alpha Fund")`/`MatchFund::new("Beta Fund")` (case
     originale, non maiuscolizzato). Un `Fund` non espone in nessun modo il nome nella sua
     scrittura originale (per design, M2/M7: `Fund::name()` è sempre maiuscolo); l'`expected` del
     test sembra costruito senza questo vincolo in mente.
  3. `formats_utils::text_filter::standard_funcs::tests::text_filter_sfdr_article::
     prefix_stripping::literal_prefixes_are_applied_before_regex_prefixes` — il commento del test
     ragiona assumendo che la rimozione dei prefissi letterali sia **ancorata all'inizio della
     stringa** ("il letterale non troverebbe più nulla da togliere all'inizio"), ma sia il
     doc-comment del modulo sia il riferimento Python (`fund_name = fund_name.replace(prefix.as_str(),
     "")`, verificato in `packages/freeports_core/src/formats_utils/text_filter/standard_funcs.rs`)
     specificano una sostituzione **non ancorata** (`str::replace`, ovunque la sottostringa
     compaia). Con la semantica corretta (e verificata contro il riferimento) il contenuto finale è
     `"Acme Fund"` (che matcha correttamente il fondo-investimento noto), non `"Prefix: Acme
     Fund"` come asserito dal test.

  In tutti e tre i casi l'implementazione è stata verificata contro il riferimento
  `freeports_core` (dove esiste) e contro gli altri test dello stesso file (che restano verdi):
  nessuna modifica di produzione risolverebbe questi tre casi senza violare un invariante
  verificato altrove o divergere dal riferimento senza una ragione documentata.

  **Correzione (2026-08-24)**: i tre fixture sono stati corretti (non il codice di produzione),
  dopo un riscontro indipendente della sessione orchestratrice contro `freeports_core` che ha
  confermato l'analisi dell'`implementer` in tutti e tre i casi:
  1. Il fixture di `interprets_amounts_as_floats_when_configured_so` ora imposta anche
     `net_assets` a `"800,5"` (oltre a `tot_assets = "1.000,5"`), così l'equazione contabile
     torna bilanciata a `1000.5` mentre resta esercitato il percorso di conversione a float.
  2. `builds_the_same_block_as_the_standard_txt_blk_helper` ora deriva il proprio set `expected`
     chiamando `MatchFund::new` sul `Fund::name()` reale degli stessi `Fund` usati in `previous`
     (non da letterali indipendenti), cosí non può più divergere dalla scrittura maiuscola che
     l'implementazione produce davvero.
  3. `literal_prefixes_are_applied_before_regex_prefixes` è stato riprogettato con un caso che
     dimostra davvero la precedenza (letterale non ancorato "Foo " + regex ancorata "^Extra Foo "
     su input "Extra Foo Bar" → "Extra Bar"), invece di un caso in cui i due ordini di
     applicazione producevano per caso lo stesso risultato scorretto.

  `cargo test`: 2092 test unitari passati (0 falliti) + 34 d'integrazione (10+22+2) verdi;
  `cargo clippy --all-targets`: solo il warning preesistente e documentato di
  `select/pdf_line/area.rs` (M3), nessun altro. M8 è ✅ chiusa senza riserve.

  **Decisioni prese durante l'implementazione (non richiedevano conferma):**
  - `output::routines` split in `accumulate.rs`/`write.rs` (più `routines.rs` come solo `pub mod`
    + re-export), come raccomandato da `agent-memory/M8-implementation-plan.md` §1: un enum
    d'errore per fase (`AccumulateError`/`WriteFilesError`) invece di un unico enum gigante.
  - `output::routines::write` scrive ogni CSV con intestazione **sempre presente** anche a zero
    righe: il crate `csv` scrive l'intestazione solo alla prima `.serialize()`, quindi mai se non
    ci sono righe — risolto disabilitando l'auto-intestazione (`has_headers(false)`) e scrivendola
    esplicitamente prima di ogni riga.
  - `investments_add_infos.yaml` usa `serde_yaml` direttamente su
    `TransformedTables::additional_infos` (un `BTreeMap`, che deriva `Serialize`): l'output
    coincide byte-per-byte con quello preteso dai test senza bisogno di riprodurre a mano il
    formato PyYAML del riferimento (non più un requisito di fedeltà, `files_schema`/`routines` non
    sono moduli verbatim).
  - `output::routines::accumulate` conferma leggendo il riferimento che `assets_managers` è
    davvero un'unica tabella con un solo indice per nome condiviso fra `ManagementCompany` e
    `InvestmentsManager` (verificato contro `Accumulator::manager_index` di `freeports_core`, non
    assunto).
  - `TextFilterSfdrArticleStandard::new`/`TextFilterAssetsStandard::new` validano i pattern regex
    a costruzione (`StandardFuncsError::InvalidPattern`), non a chiamata; `TextFilterAssetsStandard`
    valida anche che `date_regex` abbia esattamente un gruppo catturante a costruzione
    (`Regex::captures_len()`), evitando un panic su `.at(1)` a runtime.
  - `DeserializerAssetsStandard::default()`'s `interpret_int = true` resta **non verificato**
    contro un formato reale in `analysis_finance_reports_formats` (già segnalato dal test-writer
    nel doc-comment del modulo): non risolto in questa sessione, resta un'estrapolazione dal
    default di `DeserializerInvestmentStandard`.

  Stato finale verificato: `cargo test` — 2089 test unitari passati, **3 falliti** (vedi sopra,
  bug di fixture non di produzione), 0 ignorati; più 34 test d'integrazione (10
  `algorithm_end_to_end.rs` + 22 `formats_repo_loading.rs` + 2 nuovi `output_routines.rs`) tutti
  verdi; `cargo clippy --all-targets` — il warning preesistente e documentato di
  `select/pdf_line/area.rs` (M3), più un secondo warning (`cloned_ref_to_slice_refs`) dentro uno
  dei 3 test sopra elencati, anch'esso non correggibile senza toccare codice di test; `grep -rn
  "todo!()" src/ tests/` — nessun risultato.

- 2026-08-24 — M9 chiusa (`agent-memory/M9-implementation-plan.md`). Porta l'intero `cli` —
  `conf_parse::DocumentSpec`, `partial_config`, `config_locations::{cmd,env,file}`,
  `freeports_config::validate`, `batch`, `job`, `output`, `run::execute` — più `main.rs` come vero
  entry point (non più uno stub), e due moduli non assegnati a nessuna riga di `PLAN.md` §11
  (`input::companies_db`, `input::download`, porting da `packages/freeports_core/src/input/
  {companies_db.rs,download.rs}` con `csv`/`ureq` al posto di `polars`/PyO3, pull-in per Q1
  sotto).

  **Le otto decisioni Q1-Q8 dell'utente** (2026-08-24, `agent-memory/M9-implementation-plan.md`
  §0), riportate per intero in `PLAN.md` §13, tabella "Confermato dall'utente":
  - **Q1** — `input::companies_db`/`input::download` entrano nello scope di M9: confermato sì,
    incluse le correzioni di bug già trovate in quel porting (`bud`/regex non ancorato, ordine di
    `companies.csv` preservato non alfabetico).
  - **Q2** — dipendenza per la ricerca XDG/Windows delle directory di configurazione: confermato
    sì, nuova dipendenza — l'utente ha scelto di aggiungerne una (`dirs`, v6), ribaltando la
    raccomandazione "tutto a mano". Copre solo il tier utente (`dirs::config_local_dir()`); il
    tier di sistema (`XDG_CONFIG_DIRS`/`/etc` POSIX, `%PROGRAMDATA%`/`%SystemRoot%` Windows) resta
    hand-rolled, nessun crate candidato lo copriva.
  - **Q3** — estensione della semantica multi-documento a `file`/`env`: confermato, andando oltre
    la raccomandazione originale (solo `file`) — sì anche su `env`, esplicitamente richiesto
    dall'utente. `file` guadagna la chiave YAML `reports:`, `env` guadagna `FREEPORTS_REPORTS`
    (stesso separatore `|` del CSV di batch, `DOC_SPEC_SEPARATOR` promosso a costante condivisa).
    Le forme singolari (`url:`/`pdf:`, `FREEPORTS_URL`/`FREEPORTS_PDF`) restano come zucchero
    sintattico; specificare sia la forma singolare sia quella plurale sulla stessa sorgente è un
    errore di configurazione esplicito, non un override silenzioso.
  - **Q4** — `<path>:<name>:` (due punti finale, nessuno schema url rilevato): confermato errore
    tipizzato, `DocumentSpecError::TrailingColonWithoutUrl`, mai un panic né una reinterpretazione
    silenziosa — un ramo che il riferimento Python non gestisce affatto (andrebbe in `TypeError` a
    runtime).
  - **Q5** — scala di verbosità (`core::tracing_setup::Verbosity`, **riapre M0**): vedi il
    paragrafo dedicato sotto, è la decisione più importante da questa milestone.
  - **Q6** — `OutFlags::separate_out` (**riapre M8**): vedi il paragrafo dedicato sotto.
  - **Q7** — concorrenza dei job in modalità batch (`N_WORKERS`): confermato sequenziale per M9,
    come raccomandato; `n_workers` resta validato/passato a valle senza vera parallelizzazione,
    annotato come voce trasversale aperta (vedi sopra).
  - **Q8** — `TARGET_LISTS` senza alcuna sorgente che lo imposti: confermato errore esplicito
    (`FreeportsConfigError::NoTargetLists`), **ribaltando** la raccomandazione originale (default
    silenzioso a lista vuota). Una lista vuota impostata esplicitamente da una sorgente resta
    invece valida: l'errore riguarda l'assenza di una sorgente, non il contenuto della lista.

  **Q5 — riapertura di `core::tracing_setup::Verbosity` (M0, chiusa) su autorizzazione diretta
  dell'utente — la decisione più importante di questa milestone da documentare con precisione,
  perché la risposta reale diverge da entrambe le opzioni originariamente proposte dalla sessione
  orchestratrice.** Il piano M9 (§0 Q5) aveva formulato la domanda come scelta fra "tenere il
  modello a 4 livelli a conteggio di flag di M0" o "replicare il modello a delta 0-5 del
  riferimento Python". L'utente ha rifiutato entrambe le cornici e ha specificato direttamente il
  comportamento voluto: una semplicità ancora vicina ai 4 livelli, ma con `-v`/`-q` come manopole
  **indipendenti**, default che mostra Warn+Error, `-q` che riduce a solo errore, `-qq` che
  silenzia tutto, `-v` che aumenta ulteriormente. Implementazione finale: `Verbosity` passa da 4
  varianti (`Warn, Info, Debug, Trace`) a 6 (`Silent, ErrorOnly, Warn, Info, Debug, Trace`), con
  `ORDER: [Verbosity; 6]` e `DEFAULT_INDEX: usize = 2` (Warn). `from_flag_count` (M0) è stata
  **rimossa, non deprecata**: `Silent` non ha un `tracing::Level` corrispondente, quindi neanche il
  vecchio metodo `level()` poteva restare così com'era — sostituito da `level_filter() ->
  LevelFilter`. Nuovo `from_verbose_and_quiet_counts(verbose: u8, quiet: u8) -> Verbosity` calcola
  un offset netto con segno (`verbose - quiet`) rispetto a `DEFAULT_INDEX`, clampato a `[0, 5]`:
  `-v`/`-q` sono manopole indipendenti, senza errore in caso di combinazione (una divergenza
  deliberata dal riferimento Python, che tratta `-v`/`-q` come mutuamente esclusivi — è
  un'istruzione diretta ed esplicita dell'utente, non una scelta di porting). Verificato
  empiricamente (non solo per ispezione) che `LevelFilter::WARN` (il default) ammette davvero sia
  eventi `WARN` sia `ERROR`, soddisfacendo "di default vedo Warn ed Err".

  **Un bug reale trovato nella test suite di M9 stessa, che ha richiesto la propria indagine e una
  conferma separata dell'utente — documentato come voce a sé, perché è importante.** Il
  sottomodulo originale di `test-writer`,
  `core::tracing_setup::tests::stderr_layer_observable_filtering` (4 test), usava un disegno
  strutturalmente rotto: aggiungeva un secondo layer "spia" (`Counter`) come **fratello** di
  `stderr_layer` sullo stesso `tracing_subscriber::registry()`, e asseriva che il `Counter`
  avrebbe osservato solo gli eventi passati attraverso il `.with_filter(...)` di `stderr_layer`.
  Non è così: `Filtered<L,F,S>` (prodotto da `.with_filter()`) filtra **solo** il layer specifico
  che avvolge — un layer fratello aggiunto con una `.with(...)` separata riceve ogni evento
  dispacciato indipendentemente dal filtro di un altro layer, per come `tracing_subscriber` compone
  i layer. Il problema era aggravato da un secondo difetto: la cache di interesse per-callsite di
  `tracing` è globale al processo, e i 4 test invocavano le stesse identiche macro alla stessa riga
  sorgente (`tracing::error!("e")` ecc.) da thread di test paralleli con dispatcher `with_default`
  diversi, in corsa su quella cache condivisa — riprodotto empiricamente (`cargo test --lib
  core::tracing_setup::tests::stderr_layer_observable_filtering -- --nocapture`, eseguiti i test
  direttamente, confermato che `silent_shows_nothing_at_all_not_even_error` riceveva tutti e 5 i
  livelli di evento invece di zero, e altri due test fallivano in modo analogo). La causa radice è
  stata confermata non risolvibile con nessuna modifica alla logica di produzione di
  `stderr_layer`. **L'utente ha visto questa esatta analisi ed è stato interpellato su come
  procedere (`AskUserQuestion`), scegliendo "riscrivere i test con un writer iniettabile" invece di
  "eliminare questi 4 test e affidarsi solo ai test puri sulla mappatura di `level_filter()`".**
  Correzione implementata: aggiunto un punto d'innesto minimale e non invasivo nel codice di
  produzione — il corpo a una riga di `stderr_layer` è stato rifattorizzato per delegare a un nuovo
  `fmt_layer_with_writer<S, W>(writer: W, filter: LevelFilter, ansi: bool)` privato e generico
  (stesso file, `src/core/tracing_setup.rs`), con `stderr_layer` stesso invariato nel comportamento
  osservabile (usa ancora stderr reale, ANSI attivo). I 4 test sono stati riscritti per iniettare
  uno `SharedBuffer` (un `MakeWriter` sostenuto da `Arc<Mutex<Vec<u8>>>`) attraverso lo stesso
  punto d'innesto e asserire sull'output formattato realmente catturato invece che sul conteggio di
  un layer fratello, più uno `static SERIAL: Mutex<()>` che serializza solo questi 4 test fra loro
  (sono gli unici nel processo a condividere quell'esatto callsite) per eliminare la race sulla
  cache di interesse invece di limitarsi a nasconderla. Verificato eseguendo gli stessi 4 test 8
  volte di fila senza fallimenti. Questa correzione è stata fatta direttamente dalla sessione
  orchestratrice (non da un subagent), dopo che l'`implementer` aveva già indagato in autonomia,
  raggiunto la stessa conclusione sulla causa radice con le proprie riproduzioni isolate fuori dal
  crate, e correttamente rifiutato di toccare il file di test per la regola "mai modificare i test
  in autonomia" — segnalando il problema per la revisione invece di agire, esattamente come
  previsto.

  **`output::routines::write` riaperto (M8, chiusa) — `OutFlags::separate_out`.** Per Q6 (l'utente
  ha confermato la raccomandazione del piano): `OutFlags` guadagna un campo `separate_out: bool`:
  quando impostato, `write_files` scrive un CSV per report/documento (chiave
  `DocumentOutcome::id`/`format`) invece di un unico file combinato, sostituendo il concetto di
  `prefix_out` eliminato per `targets/2_multireport_support.md`. `SingleFile`/`Structured`/
  `OutFlags::compressed` — in precedenza stub d'errore tipizzati fin da Q1.2 di M8 — sono ora
  implementati per intero (compressione via le dipendenze `flate2`+`tar` già presenti).

  **Nuove dipendenze in `Cargo.toml`**: `dirs = "6"` (Q2) e `clap = { version = "4", features =
  ["derive"] }` per `CliArgs`. Rende **stale** `PLAN.md` D10 ("`thiserror` come unica nuova
  dipendenza 'di comodo'") — annotato come nota in `PLAN.md` §13, non riscritto silenziosamente.

  **Alcune decisioni implementative minori, prese dall'`implementer` per far passare i test già
  scritti da `test-writer`, non domande di design aperte:**
  - `--workers` sulla CLI clap ha bisogno di `allow_hyphen_values = true`, altrimenti `--workers
    -1` viene rifiutato da clap stesso come flag sconosciuto invece di raggiungere
    `CmdConfigError::InvalidWorkers`.
  - Il doc-comment di `cli::job` diceva in origine che `JobError::MissingInputDbPath` scatta ogni
    volta che `target_lists` è non vuoto senza `--db-directory`; il test d'integrazione
    `cli::run::tests::python_boundary::a_full_non_batch_invocation_writes_the_regular_profile_csvs_to_disk`
    (che passa `--target-list` senza `--db-directory` e si aspetta successo) ha deciso
    diversamente: un `input_db_path` assente produce ora zero target companies invece di un
    errore; il doc-comment è stato corretto di conseguenza e `JobError::MissingInputDbPath` resta
    dichiarato per stabilità dell'API ma è attualmente irraggiungibile.
  - `cli::freeports_config::validate_document_specs` distingue un path inesistente come
    "directory" o "file da scaricare" tramite presenza/assenza di un'estensione, dato che
    `targets/conf_parse.md` non copre esplicitamente questo caso limite.

  Stato finale verificato (tutto ri-verificato direttamente dalla sessione orchestratrice, non
  solo riportato da un subagent): `cargo test` — 2428 test unitari + 63 d'integrazione (10
  `algorithm_end_to_end.rs` + 29 `cli_config.rs` + 22 `formats_repo_loading.rs` + 2
  `output_routines.rs`) tutti verdi, 0 falliti, 0 ignorati (venv attivo richiesto:
  `source analysis_finance_reports/venv/freeports-dev/bin/activate`, per i sottomoduli
  `python_boundary` di `cli::job`/`cli::run`/`input::document`); `grep -rn "todo!()" src/ tests/`
  — nessun risultato; `cargo clippy --all-targets` — pulito salvo il solito warning preesistente e
  documentato di `select/pdf_line/area.rs` (M3) più 5 warning **nuovi**, tutti confermati
  strettamente dentro blocchi `#[cfg(test)]` (4 `redundant_closure` in
  `cli/config_locations/env.rs`, helper di test che avvolgono `catch_unwind(|| load())`, 1
  `ptr_arg` in un helper di solo test `display(p: &PathBuf)` di `cli/conf_parse.rs`) — non
  correggibili senza toccare codice di test, stesso trattamento del warning di solo-test di M8.
  `main.rs` è un vero entry point (`CliArgs` parsato con clap, `tracing_setup::init` chiamato
  presto, `cli::run::execute` invocato, errori mappati su un exit code non-zero); `freeports
  --help` gira e lista tutti i flag reali; `api::cli` riesporta `CliArgs`/`execute`/`CliError`,
  chiudendo `PLAN.md` §9.

## M10 — API Python e confronto con `freeports_core` (in corso)

### Cosa esiste ora

Il crate espone la sua **API Python**: `src/python/`, un `#[pyo3::pymodule] mod freeports` che
`pyproject.toml` compila come `cdylib` e `maturin develop` installa. Il pacchetto Python **è**
l'estensione: non c'è più né `freeports._native` né `freeports._internals` (i due livelli privati
del vecchio `freeports_core`), e un test lo verifica. I sottomoduli annidati
(`freeports.core`, `freeports.utils.pdf_extract`, ...) sono registrati in `sys.modules`
all'inizializzazione, perché un'estensione compilata non è un package.

L'albero è **solo shim**: newtype `#[pyclass]` e `#[pyfunction]` che convertono argomenti e
chiamano le routine native. Nessuna logica di dominio, e nessun attributo PyO3 sul codice
esistente — `PLAN.md` §14 vuole PyO3 confinato al bordo, e il bordo è quel modulo.

`freeports_dev` è migrato ai percorsi pubblici. La serializzazione delle fixture di test si è
spostata in `freeports_dev/serialization.py`: è logica di test, il cui unico consumatore è
`freeports-dev`, e non ha più un posto dentro `freeports` ora che `_internals` non esiste.

**Il repo formati non è stato modificato.** Nemmeno gli import: erano già tutti sui percorsi
pubblici. Ogni divergenza di firma è stata assorbita nel layer shim o si è rivelata un bug del
port.

### Confronto con il motore Python, sul repo formati reale

`pytest tests/formats` (21 formati, 259 test): **248 passati / 11 falliti**, contro la baseline
del motore Python **252 / 7** misurata prima di toccare qualsiasi cosa.

**Tutti i CSV di business coincidono, su tutti e 21 i formati**: `investments`, `funds`,
`funds_assets`, `assets_managers`, `investments_managers_to_funds`, `funds_change_name`,
`funds_sfdr_classification`, `funds_esg_indicators` e `investments_add_infos.yaml`.

Gli 11 fallimenti sono di due sole specie:

1. **7 test d'integrazione, solo su `.log.csv`** — CARNE-EN23, DANSKEINVEST-EN24, FINECO-EN23@IR,
   MEDIOLANUM-EN24, MEDIOLANUM-ES24.B, MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.C. Sono **gli stessi 7
   formati che fallivano in baseline** (là per un mismatch di dtype su una colonna vuota), e sono
   esattamente i 7 le cui fixture `.log.csv` non sono vuote. Il layer CSV di `tracing` funziona e
   scrive righe, ma il motore non porta nell'evento il contesto che il riferimento ci metteva
   (`Page`, `Matched Company`, `Company`) e consolida in una riga sola le tre che il riferimento
   emetteva per ogni campo perso. È il buco già annotato a M9 ("campo dedicato per la colonna
   `Matched Company`"), non una regressione di M10.
2. **4 fixture a pagina singola diventate obsolete** — ANIMA_SICAV-EN24 `text_filter[23]`,
   KAIROS-EN23 `text_filter[30]`/`[61]` e `deserialize[104]`. In tutti e quattro il blocco `FUND`
   registrato nella fixture conserva un suffisso `(in EUR)` che il modulo d'autore toglie
   (`txt_blk.content = fund_remove_regex.sub("", txt_blk.content)`); il motore lo toglie davvero,
   la fixture no. Che la fixture sia quella vecchia e non il motore a sbagliare lo dice il test
   d'integrazione degli stessi due formati, che passa: `investments.csv` non contiene il suffisso.
   Si risolvono con `freeports-dev make-tests`, non con una correzione del codice.

### Bug del port trovati e corretti dal confronto

Nessuno di questi era visibile dai test unitari: sono emersi solo facendo girare il motore su
documenti veri.

1. **`geometrical_indexes` col default sbagliato** (`formats_repo/structured/investments.rs`).
   `unwrap_or(false)` invece di `unwrap_or(true)`: il riferimento riempie l'argomento solo se la
   cella CSV non è vuota e lascia altrimenti il default della firma, che è `true`. Con `false` i
   campi si indicizzano sulla lista *piatta* dei blocchi invece che sulla griglia, e ogni riga con
   il nome andato a capo sposta le colonne di uno. Era **fatale**: abortiva l'intero documento.
2. **Il confine verso i pipe d'autore passava dizionari** (`formats_repo/unstructured/py_pipe.rs`).
   La forma duck-typed di D-M7-3 era giusta finché non esistevano le classi; ora che esistono, un
   modulo d'autore che scrive `pdf_blks[0].content` — la maggioranza — andava in `AttributeError`.
   Il confine passa e riceve le classi vere; la forma a dizionario resta accettata **in entrata**.
   Cade con essa il limite di D-M7-3: le varianti tipizzate di `BlockValue` non degradano più a
   stringa.
3. **I metadati di un blocco erano una copia, non il contenitore.** Il codice d'autore li muta in
   place (`txt_blk.metadata["fund"] = ...`, in ANIMA_SICAV-EN24 e KAIROS-EN23) e la modifica
   spariva in silenzio. `PdfBlock`/`TextBlock` tengono ora un `Py<PyDict>` vivo, come il
   riferimento, e la mappa nativa si ricava da lì.
4. **`PdfBlock`/`TextBlock` avevano l'ordine degli argomenti invertito** rispetto al riferimento
   (`(type_block, metadata, content)`, che i moduli d'autore scrivono posizionale), `TextBlock` non
   era mutabile e mancava `TextBlock.from_content`.
5. **`SfdrArticle` esponeva i nomi Rust** (`Art6`) invece di quelli pubblici (`ART_6`), che sono
   quelli che il codice d'autore scrive e quelli con cui le fixture nominano la variante.
6. **`run_job` non scriveva `.log.csv`.** Il layer esiste ma lo installava solo `main.rs`, nella
   cwd. Ora `run_job` lo installa per chiamata (`with_default`, non `set_global_default`: pytest
   chiama `run_job` una volta per formato) nella cartella di output.
7. **`investments_add_infos.yaml`: la data di scadenza usciva senza apici.** `serde_yaml` emette
   `maturity: 2025-09-22` come scalare nudo, che per `yaml.safe_load` è un `datetime.date` e non
   una stringa. Tre asserzioni di test M8 pinnavano la forma sbagliata.
8. **Un `dict` restituito da un pipe `deserialize` veniva smontato nelle sue chiavi**, perché il
   risultato veniva spacchettato con `try_iter` invece che solo su `list`/`tuple`.
9. **`PageParseFail` sollevata da un pipe d'autore era fatale.** Diventa `PipeError::PageParse`,
   l'unica variante non fatale: è la semantica del riferimento, e `eurizon_it24.py` la usa.
10. **`DeserializerAssetsStandard` accettava un `bool` dove il riferimento accetta un callable.**
    Due moduli d'autore passano una lambda (`num_converter=lambda txt: 0 if txt == "-" else
    to_int(txt)`). I due convertitori sono ora `Arc<dyn Fn...>`; `new(interpret_int, ...)` resta
    identica e nessun chiamante Rust cambia.

### Cosa resta aperto

- **`.log.csv` con il contesto del riferimento.** Serve propagare `Page`/`Matched Company`/
  `Company` fino all'evento `tracing`, e decidere se riprodurre le tre righe per campo perso del
  riferimento o tenere la riga sola consolidata di adesso. È una scelta di design, non una
  correzione meccanica: da concordare.
- **Rigenerare le 4 fixture a pagina singola obsolete** con `freeports-dev make-tests`.
- **Benchmark** (`PLAN.md` §11, M10) e docs prosa versionate.
