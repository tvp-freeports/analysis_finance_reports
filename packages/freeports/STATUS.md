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
| M4 | `text_filter` + `deserialize` | 🟡 in corso | Fatto: `deserialize::cast` (tutte le 10 funzioni + `is_numeric_shape`), `text_filter::matcher`, `text_filter::standard_txt_blk_builders`, il sottoinsieme autosufficiente di `standard_funcs` (`TextFilterPageClassifyStandard`, `extract_currency_from_text`, `DeserializerPageClassifyStandard`). Restano deferite ~10 funzioni di `standard_funcs` che dipendono da `FilterData`/`Extracted` (M5) o `output::classes` (M8) — scelta utente esplicita (opzione C), vedi sotto |
| M5 | Motore (pipeline/algorithm) | ⬜ da fare | bloccata dalla domanda aperta su `FilterData` (`PLAN.md` §13) |
| M6 | `input::document` | ⬜ da fare | |
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

## Domande aperte

Vedi `PLAN.md` §13. Le domande di M1 (`int_value()`, panic di `Set::Universe / _`), quella
sollevata da M2 (riferimenti promise pendenti) e le cinque sollevate durante M3 (vedi sopra) sono
state confermate dall'utente. La domanda sulla portata di M4 (opzioni A/B/C per le ~10 funzioni di
`standard_funcs` dipendenti da M5/M8, `agent-memory/M4-implementation-plan.md` §0) è stata risolta
il 2026-08-23: l'utente ha scelto l'opzione C (vedi voce sopra) — non è più una domanda aperta, è
lavoro deferito. Restano aperte solo le domande pre-esistenti non ancora toccate: semantica di
`FilterData` (blocca M5), rigenerazione dei fixture `freeports-dev` (M8 o M10), campo dedicato per
la colonna `Matched Company` del `.log.csv` (non blocca nulla), più le due voci trasversali aperte
durante M3 elencate sopra.

**Lavoro deferito, non una domanda aperta**: le ~10 funzioni di `standard_funcs` elencate nella
voce del 2026-08-23 sopra restano da implementare non appena M5 introduce `FilterData`/`Extracted`
(e, per quelle che dipendono da `output::classes`, non appena M8 esiste) — elenco esatto in
`agent-memory/M4-implementation-plan.md` §0.
