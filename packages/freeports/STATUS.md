# `freeports` — stato: parallelizzazione, logging, documentazione, correzioni

File di continuita' fra sessioni. **Va aggiornato alla chiusura di ogni passo**, prima di
considerare il lavoro finito. Il piano e' in `PLAN.md`; qui c'e' solo *dove siamo*.

Lo stato della riscrittura precedente (M0..M10, tutte chiuse) e' recuperabile da git:
`git show 13284baa:packages/freeports/STATUS.md`.

## Stato per passo

**Fase P chiusa al 2026-08-31**: P0/P1/P2/P5 implementate, P3/P4 chiuse senza implementazione
perche' la misura non le giustifica. Restano aperte solo la fase D (documentazione) e le
segnalazioni Q-P0/Q-P2b/Q-P5, che non bloccano nulla.

| # | Passo | Stato | Note |
|---|---|---|---|
| **F1** | Tutto in inglese (identificatori e test) | ✅ chiusa | I 6 file concentrati (~187 nomi di funzione/test) sono fatti: `core/{promise,promisable,promise_resolution,classes,classes/value,match_fund,normalization}.rs`. Poi inventario sul resto del crate (script Python: tokenizza ogni identificatore, esclude righe di commento e contenuto delle stringhe, confronta per token esatto — non substring — contro un vocabolario italiano curato, cosi' da non intercettare falsi positivi come italiano "classe/classi" dentro il legittimo inglese "class/classify/PageClass"; verificato contro la versione pre-traduzione di `promise_resolution.rs` per conferma che lo scanner funziona). Risultato: **zero identificatori italiani residui** in tutto il crate, incluse `python/*.rs` e `api.rs` (l'area di confine/API pubblica — controllata comunque, per completezza, ma non serviva alcuna rinomina). Le uniche occorrenze italiane rimaste sono doc-comment/commenti `//` (fuori perimetro, vanno in D2) e contenuto letterale di stringhe di test/fixture (dati, non identificatori — es. `"manco"` come nome di formato inventato nei test, `"testo"` come contenuto di stringa di esempio). `cargo test --lib` -> 2468 passati, 6 falliti (stessi 6 falliti pre-esistenti e indipendenti: attivazione venv/`fitz` mancante, vedi `AGENTS.md`) |
| **F2** | Ambiguita' `BlockValue::List` nella multimap delle promesse | ✅ chiusa | Opzione (a): `FlatPromiseMap.entries` da `BTreeMap<String, BlockValue>` a `BTreeMap<String, Vec<BlockValue>>` — il contenitore non e' piu' confuso col contributo. Piano, revisione critica e note di consegna in `agent-memory/F2-implementation-plan.md`. `cargo test --lib` -> **2492 passati, 0 falliti** (venv attivo); `cargo test` -> **65 d'integrazione passati, 0 falliti** (12+29+22+2). `pytest tests/formats` (repo formati, motore ricompilato con `maturin develop --release`) -> **259 passati, 0 falliti**; `out/**` verificato **byte-identico** prima/dopo (checksum SHA-256 su 308 file) — nessuna regressione sui 26 formati. Vedi "Decisioni prese" per la rottura di API pubblica e le lacune segnalate |
| **F3** | Fixture a pagina singola rigenerate, `_LEGACY_MODULES` rimosso | ✅ chiusa | 76 pagine x 3 file = 228 JSON rigenerati in 26 formati via `freeports-dev make-tests`, piu' 35 `filter_data.json` con percorsi di modulo riscritti a mano (stessa mappa dello shim rimosso — non toccati da `make-tests`, che li legge soltanto). `_LEGACY_MODULES`/`_resolve_module` rimossi da `serialization.py`. Due bug scoperti e corretti durante la rigenerazione, vedi "Decisioni prese". Verifiche finali: **zero** occorrenze `_internals`/`_native` residue in tutto `tests/formats` (prima: 175); `cargo test --lib` -> **2492 passati, 0 falliti** (venv attivo); `cargo test --test '*'` -> **65 passati, 0 falliti** (12+29+22+2); `pytest tests/formats` -> **259 passati, 0 falliti**; `out/**` **byte-identico** prima/dopo (checksum SHA-256 su 308 file) |
| **L1** | Nuovo schema `.log.csv` (colonna `Activity`, coordinate generalizzate, righe ordinate) | ✅ chiusa | Piano/critic/test-writer/implementer in `agent-memory/L1-implementation-plan.md`. `cargo test --lib` -> **2511 passati, 0 falliti** (venv attivo); `cargo test` -> **65 d'integrazione passati, 0 falliti** (12+29+22+2); `pytest tests/formats` -> **259 passati, 0 falliti** dopo la rigenerazione dei 31 `.log.csv`. Dettagli in "Decisioni prese" |
| **L2** | Strumentazione capillare, 7 aree | ✅ chiusa | **7/7 aree chiuse** (`cli` -> `input` -> `formats_repo` -> `core` -> `formats_utils` -> `output` -> `commons`), piu' tutti gli specchi `python/` (inclusi i 5 residui senza mappatura 1:1 su una singola area — `interfaces.rs`/`pipes.rs`/`api.rs`/`consts.rs`/`convert.rs` — ripiegati sull'ultimo passo `commons` come chiusura). Aggiunto l'intero vocabolario `Activity` che mancava (`PLAN.md` §3 L1): `formats_repo[<path>]`/`format[<nome>]`, `classify`/`step[<n>]`/`class[<page_class>]`/`pipeline[<nome>]`/`pdf_extract`/`text_filter`/`deserialize`/`pipe[<nome>]`, `write[<file>]`. Corretto il residuo italiano `"pagina saltata"` -> `"page skipped"` (area `core`, come previsto) e altri due mislivellamenti trovati in revisione (`unstructured/py_pipe.rs`: `info!`->`warn!`; `deserialize/standard_funcs.rs`: `error!`->`warn!`). Verificato indipendentemente ad ogni area, mai una regressione: `cargo test --lib` -> **2511 passati, 0 falliti**; `cargo test` -> **65 d'integrazione passati, 0 falliti** (12+29+22+2); `cargo clippy --lib --tests` invariato (6 warning pre-esistenti, mai aumentati). Un bug ereditato segnalato e lasciato com'e' per decisione esplicita dell'utente (`find_config`); due note strutturali aperte per una sessione futura, non bloccanti. Dettagli area per area in "Decisioni prese" |
| **L4** | Messa a punto del logging dopo la revisione dell'utente su L2 | ✅ chiusa (2 giri) | Revisione dell'utente su L2 chiusa: troppi log, forma ripetitiva (`class{class=investments}`), programma ~100x piu' lento, messaggi non contestualizzati, pagina assente in TextFilter/Deserialize. Diagnosi misurata e piano in `agent-memory/L4-logging-tuning-plan.md`. Risultato: **13,0 s** contro i **19 minuti** di partenza sullo stesso job (EURIZON-EN23.A, 1140 pagine, verbosita' di default); `.log.csv` da **2,8 GB** a **609 righe**; `pytest tests/formats` da 388 s a **93 s**. `cargo test --lib` -> **2518 passati, 0 falliti**; `cargo test` -> **65 d'integrazione passati, 0 falliti**; `pytest tests/formats` -> **259 passati, 0 falliti** dopo la rigenerazione dei 22 `.log.csv` (dei 31 confrontati) autorizzata dall'utente; **nessuno** degli altri 277 file di `out/**` toccato, provato con checksum SHA-256 prima/dopo. `cargo clippy` invariato (gli stessi 6 warning pre-esistenti). **Secondo giro (2026-08-30)** su richiesta dell'utente: `.log.csv` spostato nella cartella di `out`, ristretto a `warn`+`error`, e nuova resa della riga su stderr (niente timestamp, livello/`Activity`/target colorati). Dettagli in "Decisioni prese" |
| **L3** | `.freeports.log.yaml` a verbosita' massima | ✅ chiusa | `YamlLogLayer`, quarto layer accanto ai tre esistenti. **Q-L3 risposta: opzione (b)** — record strutturale (`ErrorRecord`: `debug`/`display`/catena di `source()`), nessun `Serialize` derivato sui ~25 enum d'errore, funziona anche per errori di terze parti. `debug` sostituisce il `type` ipotizzato dal piano: un `&dyn Error` non sa dire il proprio tipo concreto, ma `{:?}` su un enum `thiserror` stampa gia' variante e campi (`AlgorithmLoad(UnknownFormat { format: "NOPE", known: 27 })`), che e' di piu'. Le tre decisioni lasciate aperte dal piano, risposte dall'utente: **solo `warn!`/`error!`**, file **nella cartella corrente** (e' un artefatto diagnostico, non un prodotto della corsa), **implicito in `-vvv`** senza flag dedicato. `serde_yaml` 0.9 tenuto (gia' dipendenza, gia' usato per `investments_add_infos.yaml`); la nota del piano sulla sua manutenzione resta valida ma non blocca. Perche' il file si popoli davvero, sweep di **53 siti** `error!`/`warn!` che interpolavano l'errore nel messaggio: ora vi agganciano anche `error = log_error(&e)`, il messaggio resta invariato (nessuna deriva dei fixture) e stderr non stampa piu' il campo `error` per non ripetere due volte la stessa cosa sulla stessa riga. 11 test in `tests::yaml_layer` |
| **L5** | Log su file strutturato, stderr leggibile a colpo d'occhio | ✅ chiusa | Revisione dell'utente su L3/L4 chiuse (2026-08-30), cinque richieste, tutte chiuse: (1) `freeports.log` -> **`freeports.log.jsonl`**, un oggetto JSON per riga (`JsonLogLayer`); (2) stderr **senza `target`** (il percorso del modulo), che resta invece un campo di ogni record del file; (3) `.log.csv` **mai piu' nella cartella corrente** — la causa era il fallback di `LogHandle::close`, che scattava a ogni corsa fallita prima della risoluzione della configurazione, riprodotto e rimosso (`CsvLogLayer::discard`); (4) **quattro colori** nel percorso degli span (nomi ciano, valori magenta chiaro, `/` e `[`/`]` grigio scuro, campi in dim) invece di uno solo; (5) **errore serializzato anche nel log su file**, grazie a `LogRecord`/`build_record` condivisi fra `JsonLogLayer` e `YamlLogLayer`. `cargo test --lib` -> **2550 passati, 0 falliti** (12 nuovi: `tests::json_layer`, `tests::csv_never_in_the_working_directory`); `cargo test` -> **65 d'integrazione passati**; `pytest tests/formats` -> **259 passati**, con i 31 `.log.csv` e i 277 altri file di `out/**` **byte-identici** prima/dopo (checksum SHA-256) — nessuna rigenerazione necessaria; `cargo clippy --all-targets` invariato (gli stessi 6 warning pre-esistenti, nessuno in `tracing_setup.rs`). Piano in `agent-memory/L5-structured-log-plan.md`. Dettagli in "Decisioni prese" |
| **P0** | Profilo su report reali | ✅ chiusa | Misura, non implementazione: **nessun file di produzione toccato**. Strumento in `packages/freeports/examples/p0_profile.rs` (example di cargo, fuori dal binario, da rieseguire dopo P1..P4), che legge gli `info_span!` gia' installati da L2 con un layer `tracing` che accumula il tempo per percorso di span; filtro identico a quello di produzione alla verbosita' di default, overhead sotto la soglia di rumore (profilo 0,70/2,55/17,66/21,30 s contro binario `release` 0,72/2,58/17,75/21,40 s). **Quattro** documenti invece di tre: MEDIOLANUM-ES24.B (29 pagine), UBS-EN23 (222), EURIZON-EN23 (1.140, aggiunto perche' e' l'unico grande **con** pipe Python d'autore), AMUNDI-EN24 (1.824, interamente structured). Le tre risposte: (1) **caricamento PyMuPDF 35-75% del totale**, di cui 72-93% PyMuPDF vero (verificato a parte con lo stesso `load_page`+`get_text("dict")` da Python puro); (2) **classificazione da 1:8,5 a 1:157 rispetto agli step**, e pesa solo dove e' scritta in Python; (3) **`text_filter` e' l'85-96% del lavoro del motore e dentro c'e' un solo pipe**, `TextFilterInvestmentsStandard` (Rust puro, 14-20 ms/pagina, **30-54% del tempo totale di un job**), mentre `deserialize` sta sotto lo 0,2% ovunque. Nessuna regressione: `cargo test --lib` -> **2550 passati, 0 falliti**; `cargo test --test '*'` -> **65 passati, 0 falliti** (12+29+22+2); `cargo clippy --all-targets` invariato (gli stessi 6 warning pre-esistenti, nessuno nell'example). Rapporto completo in `agent-memory/P0-profile.md`; conseguenze su P2/P4/P5 e rischio §9.3 riportate in `PLAN.md` |
| **P1** | Job/documento — processi | ✅ chiusa | **Q-P1 e Q-P2 risposte il 2026-08-30**, passo sbloccato e chiuso lo stesso giorno. Piano, checklist e scostamenti in `agent-memory/P1-implementation-plan.md`. La premessa di `PLAN.md` §4 P1 era **sbagliata** e l'ho corretta prima di iniziare: i job **non** scrivono i propri CSV, `execute` concatena i `DocumentOutcome` di tutti e fa **una sola** `write_results` con i parametri della prima configurazione risolta — per questo servono processi **con IPC dei risultati**, non processi indipendenti. Otto passi (P1.1..P1.8): derive serde su config e risultati, modulo `cli::worker` (protocollo a due file JSON), flag nascosto `--internal-worker`, pool a scorrimento con raccolta in slot indicizzati, unione dei log in ordine di job. `cargo test --lib` -> **2.608 passati, 0 falliti** (+58); `cargo test --test '*'` -> **72 passati, 0 falliti** (12+29+**7 nuovi**+22+2); `pytest tests/formats` -> **259 passati, 0 falliti**; `out/**` **byte-identico** prima/dopo (checksum SHA-256 su 308 file, nessuna rigenerazione necessaria); `cargo clippy --all-targets` invariato (gli stessi 6 warning pre-esistenti, nessuno nel codice nuovo). **Guadagno misurato: 13,34 s -> 7,08 s, 1,88x** su un batch di 2 job (i due EURIZON-EN23) con `-j 2`, tre ripetizioni stabili entro l'1%. Dettagli in "Decisioni prese" |
| **P2** | Pagina — thread rayon (step, poi classificazione) | ✅ chiusa | Piano, decisioni e scostamenti in `agent-memory/P2-implementation-plan.md`. Ordine di P0 rispettato: prima il ciclo delle pagine di uno step, poi `classify_pages`. Sei passi (P2.1..P2.6): `rayon` + nuovo `core::parallelism` (`Parallelism`, pool dedicato in `OnceLock` — **non** quello globale di rayon, che appartiene a chi incorpora il crate); `scales_with_threads()` sui tre trait dei pipe (default `true`, `false` sui tre `Py*Pipe`) e su `Pipeline`/`PipelinesBundle`; le varianti `apply_with`/`apply_multidocument_with`/`classify_pages_with`/`classify_pages_multidocument_with`, con le firme storiche che restano **sequenziali**; cablaggio `cli::run` -> `cli::job` -> `WorkerRequest.page_workers`. **Guadagno misurato** (20 thread hardware, `examples/p0_profile --pages 1` contro `--pages 20`, due ripetizioni entro l'1%): motore `apply_multidocument` **7,0x** AMUNDI-EN24 (7.197 -> 1.028 ms), **5,1x** UBS-EN23 (611 -> 119 ms), **4,4x** EURIZON-EN23 (11.682 -> 2.656 ms); end-to-end con il binario `release` su EURIZON-EN23 **2,20x** (19,81 s -> 9,02 s), il resto essendo il caricamento PyMuPDF che solo P1 tocca. MEDIOLANUM-ES24.B non guadagna nulla (182 -> 189 ms) ed **è il comportamento voluto**: ha un `deserialize` d'autore, quindi degrada a sequenziale. `cargo test --lib` -> **2.633 passati, 0 falliti** (+25); `cargo test --test '*'` -> **73 passati, 0 falliti** (12+**1 nuovo**+29+7+22+2); `pytest tests/formats` -> **259 passati, 0 falliti**; `out/**` **byte-identico** prima/dopo (checksum SHA-256 su 308 file); `cargo clippy --all-targets` invariato (gli stessi 6 warning pre-esistenti, nessuno nel codice nuovo). In più, output CSV **byte-identici** fra corsa sequenziale (`taskset -c 0`, che porta `available_parallelism` a 1) e parallela su quattro formati reali, `.log.csv` incluso dove esiste. Una nota sul **tempo** della suite pytest, che non c'entra con P2 ma va detta: gira in 241 s contro i 93 s registrati alla chiusura di L4. Verificato che non dipende da P2 — lo stesso sottoinsieme (`-k AMUNDI-EN24`) impiega **22,13 s con l'estensione costruita da `HEAD`** e **22,12 s con quella di P2**. Il rallentamento e' quindi anteriore a oggi (fra la misura di L4 e `96b7fad6`) e resta da indagare a parte. Dettagli in "Decisioni prese" |
| **P3** | Page class / pipeline dentro uno step | ❌ chiusa senza implementazione | **P0 e P2 insieme non la giustificano.** P0 aveva gia' misurato «1-2 page class per step e una pipeline dominante»; P2 ha poi preso *tutti* i thread per le pagine di ogni gruppo di class, quindi P3 vivrebbe sopra un ciclo che satura gia' la macchina — due unita' di lavoro annidate in rayon senza thread liberi a cui darle. Il caso patologico che il piano citava a sua difesa ("pochissime pagine e molte pipeline pesanti") **non esiste nel corpus**: il piu' piccolo dei 21 report reali ha 29 pagine, gia' piu' dei 20 thread hardware. E un'opzione disattivata di default non gira mai in produzione, quindi non viene mai esercitata, e resta da mantenere a ogni cambiamento di `core::algorithm`. Conseguenza su P5, identica a quella di P4: `pipelines` sparisce dallo schema invece di restare un'opzione morta. Ragionamento per esteso in `PLAN.md` §4 P3 e in `agent-memory/P5-implementation-plan.md` §0 |
| **P4** | Blocchi di `deserialize` sopra soglia | ❌ chiusa senza implementazione | **P0 non lo giustifica**: `deserialize` costa 22-27 ms su job da 17-21 s, sotto lo 0,2% del totale su tutti e quattro i documenti — azzerarlo varrebbe lo 0,1%. Sui *pipe* non si parallelizzava comunque (`PLAN.md` §4 P4). Conseguenza su P5: `deserialize_blocks_threshold` tolto dallo schema invece di restare un'opzione morta |
| **P5** | Configurazione `parallelism` | ✅ chiusa | Piano, decisioni e scostamenti in `agent-memory/P5-implementation-plan.md`. Nuovo modulo `cli::parallelism_config` (`Workers` = `Auto`/`Fixed`, `ParallelismConfig`), che **sostituisce** i due provvisori: `cli::run::{job_parallelism,page_parallelism}` sono spariti, al loro posto `resolve_parallelism`, che risolve i due livelli insieme perche' il secondo dipende dal primo. Superficie: `--workers`/`-j` (default globale di **entrambi** i livelli), `--jobs`/`--pages` (override per livello), `FREEPORTS_N_WORKERS`/`FREEPORTS_PARALLELISM_JOBS`/`FREEPORTS_PARALLELISM_PAGES`, chiavi YAML `n_workers` e `parallelism: {jobs, pages}`; tutte accettano un intero positivo **o** `auto`. **Guadagno misurato con il default** (due job grandi, EURIZON-EN23 + AMUNDI-EN24, binario `release`, due ripetizioni: 39,37/39,73 s contro 16,68/16,70 s): **39,4 s -> 16,7 s, 2,36x**, con l'output **byte-identico** a quello della corsa sequenziale. Prezzo: picco di memoria **783 MB -> ~1,2 GB**. `cargo test --lib` -> **2.681 passati, 0 falliti** (+48); `cargo test --test '*'` -> **83 passati, 0 falliti** (12+1+34+**12** di cui 5 nuovi+22+2); `pytest tests/formats` -> **259 passati, 0 falliti**; `out/**` **byte-identico** prima/dopo (checksum SHA-256 su 308 file); `cargo clippy --all-targets` invariato (gli stessi 6 warning pre-esistenti, nessuno nel codice nuovo). **Un difetto di P1 scoperto e corretto qui**, vedi "Decisioni prese" |
| **D1** | Strategia documentale (Sphinx+MyST+rustdoc vs mdbook) | ⬜ da fare | Bloccato su **Q-D2**. `docs/source/_generated/` documenta un pacchetto morto (`freeports_analysis`) e va cancellato in ogni caso |
| **D2** | Doc-comment riscritti, area per area | ⬜ da fare | 6.103 righe di doc-comment, ~2.961 in italiano. Per convenzione del workspace e' lavoro di `implementer`, non di `docs-writer` |
| **D3** | Whitepaper | ⬜ da fare | Materiale di partenza: `docs/source/{dev/code,usage/command,validation/**}.rst` + `PLAN.md` storico §2/§12/§13 |
| **D4** | Riporto e riconciliazione dei contenuti Sphinx | ⬜ da fare | ~9.000 parole di prosa esistente, 4 locali gettext |

Legenda: ⬜ da fare · 🟡 in corso · ✅ chiusa (test verdi, `STATUS.md` aggiornato) · ❌ chiusa senza implementazione (decisa da una misura, non dimenticata)

## Baseline al momento della scrittura del piano (2026-08-24, commit `13284baa`)

Misurata, non assunta:

- crate: 116 file `.rs`, 49.042 righe; 2.474 test unitari + 63 d'integrazione, verdi;
- repo formati: `pytest tests/formats` -> 259 passati / 0 falliti;
- logging: 19 siti in 10 file, 3 span (`page`, `field`, coppia societa');
- `.log.csv`: header `Page,Matched Company,Company,Field name,Row,Column,Message`;
- report di test: 21 PDF, mediana 288 pagine, media 480, massimo 1.824 (AMUNDI-EN24);
- fixture a pagina singola: 76 x 3 = 228 JSON, 175 con tag `freeports._internals`/`_native`.

## Domande aperte — nessun passo bloccato parte prima della risposta

Testo completo in `PLAN.md` §7. In sintesi:

| # | Blocca | Domanda in una riga |
|---|---|---|
| Q-F1 | F1, D2 | I doc-comment italiani vanno tradotti? (raccomandazione: si', dentro D2) |
| Q-F2 | F2 | Quale forma per la correzione delle promesse: contributi separati / variante `Multi` / opt-in? (racc.: la prima) |
| Q-F3 | F3 | ~~Rigenerazione dei 228 JSON confermata, `out/**` intatto?~~ **Risposta 2026-08-29: si'** ("procedi con F3"). |
| Q-L1 | ~~**L1, e a cascata L2/L3**~~ | ~~I 31 `out/.log.csv` si possono rigenerare, o serve una modalita' di compatibilita'?~~ **Risposta 2026-08-29: si', rigenerazione una tantum.** |
| Q-L2 | ~~L1~~ | ~~Nome/posizione della colonna span; `Activity` da sola basta a generare una riga?~~ **Risposta 2026-08-29: no, come raccomandato.** |
| Q-L3 | ~~L3~~ | ~~YAML: quando si genera, cosa contiene, record strutturale o `Serialize` sugli enum?~~ **Risposta 2026-08-30: record strutturale (opzione b); solo `warn`/`error`; solo a `-vvv`, implicito, nella cartella corrente.** |
| Q-P0 | l'ordine di tutta la fase P | **Rimandata 2026-08-30** ("procedi con P1"): resta un passo a se', dopo. **Nuova (2026-08-30, aperta da P0).** `TextFilterInvestmentsStandard` da solo e' il 30-54% del tempo totale di un job, ed e' Rust mono-thread e deterministico: si apre un passo di ottimizzazione *interna* di quel pipe prima di P1/P2? |
| Q-P1 | ~~P1~~ | ~~Processi figli ammessi, o solo thread?~~ **Risposta 2026-08-30: processi figli, con IPC dei risultati verso il padre.** |
| Q-P2 | ~~P2, P3, P4~~ | ~~Determinismo byte-per-byte confermato come vincolo?~~ **Risposta 2026-08-30: no, basta l'equivalenza semantica.** Nota: P2 e' comunque risultato byte-identico, senza dover spendere il margine concesso. |
| Q-P5 | nulla — segnalazione | **Nuova (2026-08-31, aperta da P5).** Il default `jobs: auto` rende paralleli i batch senza che nessuno lo chieda: **2,36x** piu' veloce, ma il picco di memoria passa da 783 MB a ~1,2 GB con due soli job grandi, e cresce con il numero di job concorrenti. Va bene cosi', o il default prudente (`jobs: 1`) e' preferibile e il parallelismo va chiesto? E' una riga in `partial_config::defaults`. |
| Q-P2b | nulla — segnalazione | **Nuova (2026-08-30, aperta da P2).** Il binario `freeports` esce con **SIGSEGV** dopo aver scritto correttamente tutti gli output, sul formato CARNE-EN23. Preesistente e indipendente da P2 (stesso crash sul percorso sequenziale). Si sistema, e come? |
| Q-D1 | D3 | Il whitepaper parla anche a un pubblico non tecnico? |
| Q-D2 | D1, D4 | Sphinx unico + rustdoc accanto (scartando mdbook)? Che ne e' delle 4 traduzioni? |

## Decisioni prese durante l'implementazione

*(Ogni volta che l'utente risponde a una domanda aperta, o si prende una decisione non prevista da
`PLAN.md`, va annotata qui con la data e riportata nella sezione giusta di `PLAN.md`.)*

- 2026-08-24 — Piano scritto a partire da `packages/richieste.txt`. Ordine delle fasi **F -> L ->
  P -> D**, diverso da quello della richiesta: la motivazione tecnica e' in `PLAN.md` §1 (in breve:
  gli span di `tracing` non attraversano da soli un confine di thread, quindi strumentare dopo aver
  parallelizzato significa scrivere la strumentazione due volte).
- 2026-08-24 — All'avvio di F1 la crate non compilava per un motivo indipendente: `packages/freeports_core`
  era stato cancellato dal disco (208 file, cancellazione non ancora committata) ma
  `src/commons/i18n.rs` faceva ancora `include_bytes!` sul `.mo` italiano dentro quell'albero.
  L'utente ha scelto di sistemare `i18n.rs` piuttosto che ripristinare `freeports_core` o procedere
  senza `cargo test`: il fixture binario e' stato copiato in
  `packages/freeports/src/commons/testdata/messages.it.mo` e l'`include_bytes!` punta li'. Necessario
  perche' F1 richiede `cargo test` dopo ogni modulo.
- 2026-08-29 — F1 chiusa. Dopo i 6 file concentrati, un primo tentativo di grep ingenuo su parole
  italiane comuni ha prodotto centinaia di falsi positivi (es. "classe"/"classi" dentro "class"/
  "classify"/"PageClass", o parole italiane che comparivano solo dentro doc-comment, mai nel
  codice). Sostituito con uno scanner che tokenizza gli identificatori, esclude commenti e stringhe,
  e confronta per token esatto: applicato a tutto il crate (non solo alle aree elencate in
  `PLAN.md` §8), non ha trovato altri identificatori italiani da rinominare — l'inventario "~30
  file" della stesura originale del piano risultava gia' superato dal lavoro fatto nel frattempo.
- 2026-08-29 — **Q-F2 risposta: opzione (a)**, separare contenitore da contributo. Durante la
  pianificazione e' emersa una seconda domanda non implicata da (a), **Q-F2b**: come si comporta un
  contributo `BlockValue::Null` in mezzo ad altri. Prima risposta (R2, "Null e' sempre un
  non-contributo") lasciava aperto *dove* scartarlo; la revisione di `critic` (C3) ha isolato due
  letture con effetti diversi su `get`/`iter`/lo splicing dei riferimenti. **Risposta finale: `Null`
  si scarta gia' durante `flatten`**, non solo in `fulfill` — un id con soli contributi `Null`
  sparisce dalla `FlatPromiseMap` come un id senza contributi; un riferimento a quell'id resta una
  `Promise` pendente (non eredita `[Null]`).
- 2026-08-29 — `critic` ha trovato un errore di fatto bloccante nel piano di F2 (C1: dichiarava
  verdi 5 test di `promisable.rs` che invece si rompevano) e un'analisi incompleta (C2: il "delta
  osservabile" dello splicing dei riferimenti era descritto troppo stretto). Entrambi corretti nel
  piano prima che `test-writer` scrivesse i test. Lezione: quando un piano dichiara "questo file non
  cambia" o "questo test resta verde", verificarlo leggendo il file, non fidarsi della dichiarazione
  — e' esattamente il ruolo che l'ordine `implementation-planner -> critic -> test-writer ->
  implementer` di `PLAN.md` §8 assegna a `critic`, e ha funzionato.
- 2026-08-29 — **C4**: `FromIterator<(K, V)> for FlatPromiseMap` (bound `V: Into<BlockValue>`)
  avrebbe lasciato compilare in silenzio `from_iter([("id", vec![a, b])])` come un contributo-lista
  invece di due contributi — la stessa ambiguita' di F2, riaperta alla costruzione. Rimossa l'impl;
  sostituita con `FlatPromiseMap::from_pairs(...)`, `#[cfg(test)] pub(crate)`, usata da tutti i
  moduli di test del crate che costruivano una `FlatPromiseMap` a mano.
- 2026-08-29 — **C5**: verificato che un pipe d'autore Python **non perde capacita'** potendo
  restituire solo un dict per chiamata: `PyDeserializePipe::deserialize` (`py_pipe.rs`) appiattisce
  gia' una lista di dict restituita dal pipe, e `accumulate.rs` unisce ogni `Extracted::Promises` di
  ogni pagina in un'unica `PromiseMap` che accumula per chiave. Un pipe che oggi scrive
  `return {"id": [a, b]}` ottiene lo stesso effetto (due contributi per `id`) scrivendo
  `return [{"id": a}, {"id": b}]` — meccanismo preesistente, indipendente da F2. Nessuna proposta al
  repo formati necessaria; nota aggiunta al doc-comment di modulo di `promise_resolution.rs`.
- 2026-08-29 — Verifica di non-regressione sul repo formati (`analysis_finance_reports_formats/`)
  eseguita nonostante l'albero avesse gia' **316 file non committati** in `tests/formats/**` (inclusi
  CSV `out/**`, non causati da questa sessione: mtime 2026-08-18, undici giorni prima, stato noto e
  confermato dall'utente). Verificato con checksum SHA-256 su tutti i 308 file `out/**` prima e dopo
  il run: **identici** — F2 non ha aggiunto alcuna modifica a quello stato preesistente. Il criterio
  "`git status --porcelain tests/formats` vuoto" del piano originale non era applicabile cosi'
  com'era per questo motivo, contingente allo stato del repo al momento del lavoro, non a F2.

- 2026-08-29 — **Q-F3 risposta: si'** ("procedi con F3"), presa come conferma che la rigenerazione
  copre i 228 JSON a pagina singola di tutti i 26 formati con `out/**` intatto — esattamente lo
  scopo gia' descritto in `PLAN.md` §2 F3. Baseline verificata prima di iniziare: `pytest
  tests/formats` gia' 259/0 in partenza, quindi nessun formato escluso dalla precondizione "solo se
  l'integrazione e' verde".
- 2026-08-29 — **Bug scoperto 1**: `_promise_tag` in `serialization.py` scriveva `"id": str(p)`
  invece di `"id": p.id`. Innocuo finche' un blocco PDF con una promessa nel campo `content` restava
  nel vecchio schema non taggato (il fallback di `_block_content`, mai esercitato dallo scrittore),
  ma la rigenerazione di F3 lo ha esercitato per la prima volta: la fixture riscritta portava un id
  raddoppiato (`Promise(id="title document", strict=False, multiple=False)` invece di `"title
  document"`), rompendo il confronto pdf_extract/text_filter/deserialize su 2 pagine
  (EURIZON-EN23/2 pagina 1017, MEDIOLANUM-ES24.B pagina 20). Corretto con `p.id`; non e' una scelta
  di design, il vecchio comportamento non aveva alcun consumatore legittimo.
- 2026-08-29 — **Bug scoperto 2** (root cause piu' profonda, nel crate `freeports`, non in
  `freeports_dev`): l'entita' `Fund` (`output/classes/fund.rs`) ha il campo interno serde `n_name`
  (la forma normalizzata privata) mentre il costruttore pubblico PyO3 accetta `name`.
  `__serialize_fields__()` (`python/output.rs`) e' documentato per coincidere sempre con gli
  argomenti del costruttore "per costruzione", perche' deriva dalla forma serde — vero per le altre
  sei entita', falso solo per `Fund`. Rimasto invisibile finche' le fixture con un `Fund` erano
  quelle vecchie (con la chiave `name` del modello Pydantic di riferimento); la rigenerazione di F3
  ha scritto `n_name`, rompendo la ricostruzione (`TypeError: Fund.__new__() got an unexpected
  keyword argument 'n_name'`) su 53 test. **Chiesta conferma all'utente** fra tre opzioni (rename
  serde, mappa manuale in Python, altro); scelta l'opzione root-cause: `#[serde(rename = "name")]`
  sul campo `n_name`, che ripristina l'invariante documentato senza casi speciali. La
  rinormalizzazione in `Fund::from_value` e' idempotente, quindi ricostruire dalla forma gia'
  normalizzata (ora sotto la chiave `name`) produce lo stesso `Fund`. Richiesto un giro di
  `maturin develop --release` e una seconda rigenerazione completa dei 76 trittici (la prima aveva
  scritto ancora `n_name` e l'id di promessa corrotto, prima che i due fix fossero applicati).
- 2026-08-29 — **L2 avviata, area `cli` (1/7).** Pipeline `implementer` -> `critic` -> `implementer`
  (fix mirato), non la sequenza completa di `PLAN.md` §8 perche' L2 e' uno *sweep* (poca ambiguita',
  molto volume), non una fase con ambiguita' vera: coerente con "L2 | Sweep di strumentazione, area
  per area | implementer | S" e "L2 | Verifica del rumore prodotto | critic | S" gia' previsti li'.
  `conf_parse.rs` e `partial_config.rs::resolve_singular_and_plural_reports` lasciati senza log
  nuovi per scelta (leaf condivise e senza contesto, gia' coperte dal wrap-log di ciascuno dei 4
  chiamanti — `critic` ha confermato il ragionamento). `critic` ha trovato un problema strutturale
  non locale all'area `cli`: il catch-all `tracing::error!("{e}")` di `main.rs` (gia' presente da
  L1) duplicava verbatim ogni wrap-log di area appena aggiunto, perche' tutte le varianti di
  `CliError` sono `#[error(transparent)]`. **Decisione dell'utente**: tenere entrambi i segnali ma
  con testo distinto — `main.rs` ora logga `"freeports is exiting due to the error above"` (nessun
  `{e}` ripetuto, il `eprintln!` sotto mostra comunque il messaggio all'utente), le aree continuano
  a loggare il proprio fallimento con contesto pieno. Chiude anche la lacuna di `OutputError` (unica
  variante di `CliError` senza log d'area prima del fix: `cli::output::write_results` ora segue lo
  stesso pattern wrapper/impl degli altri moduli). Altri due fix minori dallo stesso giro di
  `critic`: `JobError::MissingFormatsRepoPath` era loggato due volte (localmente e dal wrapper di
  `job::run`) — rimosso il log locale, resta solo il wrapper, come per le altre varianti; l'asimmetria
  `debug!`/`info!` fra "riuso locale" e "scarico" in `job::resolve_document_path` — allineata a
  `info!` per entrambi i rami, essendo la stessa decisione visibile all'utente. **Decisione
  dell'utente**: il cambiamento di comportamento in `env.rs::parse_bool` (accetta anche
  `yes/1/y/t`/`no/0/n/f`, non solo `true/false`, per le variabili d'ambiente `FREEPORTS_*`), trovato
  da `critic` come scope-creep estraneo al logging, **approvato cosi' com'e'** — non e' stato
  ripristinato. `cargo test --lib` -> **2511 passati, 0 falliti**; `cargo test` -> **65
  d'integrazione passati, 0 falliti** (12+29+22+2), sia dopo il primo giro sia dopo il fix.
- 2026-08-29 — **Nota per le prossime 6 aree di L2**: la regola "lingua inglese" non era esplicita
  nella convenzione di `PLAN.md` §3 L2 (lo e' per F1/D2). Durante lo sweep di `cli` e' emerso un
  messaggio di log italiano preesistente e non toccato, `"pagina saltata"` in
  `core/algorithm.rs` (~riga 295) — fuori perimetro per `cli`, da correggere quando tocca all'area
  `core`. Va aggiunta esplicitamente una regola "inglese" a `PLAN.md` §3 prima di quella sessione,
  cosi' come gia' fatto per F1/D2.
- 2026-08-29 — **L2, area `input` (2/7).** Aggiunto lo span `page` in `document.rs::load_document_pages`
  (unico punto di orchestrazione genuino dell'area — riusa lo stesso nome campo che poi userebbe
  `core::algorithm.rs`); `info!("document loaded")` (letteralmente l'esempio della convenzione),
  `debug!` di riepilogo in `companies_db.rs`/`download.rs`, `trace!` sui loop caldi per-span/per-riga
  di `page_dict.rs`/`selection.rs`. Audit regola 1: **unico** punto di assorbimento reale trovato in
  tutta l'area sono i 2 confini PyO3 di `python/input.rs` (ora `error!`) — il resto dell'area
  propaga sempre via `?`. Nessun bug trovato. `cargo test --lib` -> **2511/0**, `cargo clippy`
  invariato.
- 2026-08-29 — **L2, area `formats_repo` (3/7).** Aggiunto lo span mancante dal vocabolario
  `Activity`, `formats_repo[<path>]`/`format[<nome>]`, attorno a `formats_repo.rs::Algorithm::load`
  (nessuno span esisteva ancora per questo punto). Rivisto `unstructured/py_pipe.rs:291`: era
  `info!("author pipe could not parse the page")`, mentre il consumatore
  (`core::algorithm.rs::apply`) tratta questo caso come pagina persa — corretto a `warn!`. ~20
  `debug!` di riepilogo (`metadata.rs`, `orchestration.rs`, `structured.rs`+3, `semistructured.rs`,
  `unstructured/loader.rs`), ~7 `trace!` sui rami senza-match/dispatch per-pagina. Due scoperte
  riportate senza agire d'iniziativa: `structured/tables.rs` legge righe CSV di *configurazione*
  pipe (non e' il vero tabularizer, che vive in `formats_utils`); `orchestration.rs::get_mapping`
  non ha in realta' un ramo di fallback Python nel porting Rust (il parametro `defined_pipelines`
  lo sostituisce apposta per evitare un ciclo di moduli — gia' documentato nel suo stesso
  doc-comment). Nessun bug di comportamento trovato. `cargo test --lib` -> **2511/0**.
- 2026-08-29 — **L2, area `core` (4/7), il passo piu' grosso per span.** Corretto il residuo
  italiano gia' segnalato, `"pagina saltata"` -> `"page skipped"` (`algorithm.rs`, nessun test
  asseriva sulla stringa letterale). Aggiunti i **6 span** ancora mancanti dal vocabolario
  `Activity`: `classify` (`classify_pages`), `step[<n>]`/`class[<page_class>]` (nidificati dentro
  `apply_multidocument`, raggruppati per classe via `chunk_by` — verificato che le pagine di uno
  step sono gia' contigue per classe, non assunto), `pipeline[<nome>]` (`pipeline.rs::Pipeline::apply`
  e affini), `pdf_extract`/`text_filter`/`deserialize`/`pipe[<nome>]` (`pipeline/segment.rs`, un
  pipe alla volta anche per `deserialize`, il piu' caldo dei tre, per rispettare la lettera del
  vocabolario). `debug!` per "page classes assigned" e "promises deposited" (esempi espliciti della
  convenzione), 3 `warn!` (`promisable.rs`, promessa non risolta), 2 `trace!` (`promise_resolution.rs`,
  rispetta il ciclo caldo). `python/core.rs` strumentato ai suoi confini PyO3. **Nota strutturale
  aperta, non risolta**: lo span `document[<id>]` (da `cli/job.rs`) si chiude prima che
  `apply_multidocument` giri, perche' lo scheduling multi-documento interfoglia pagine di documenti
  diversi dentro uno stesso `step` — quindi oggi `.log.csv` non porta un prefisso `document[...]`
  da `classify`/`step`/`page` in poi nel caso multi-documento. Candidato non deciso: passare l'id
  documento come campo del gia' esistente span `page` invece che affidarsi a uno span englobante.
  Nessun bug trovato. `cargo test --lib` -> **2511/0** (venv attivo, richiesto per i test PyMuPDF).
- 2026-08-29 — **L2, area `formats_utils` (5/7), la piu' grande (23 file + 2 specchi `python/`).**
  Coppia `python/utils/*`+`python/standard_funcs/*` fatta nella stessa sessione, com'era lo scopo
  della nota di perimetro dell'utente in `PLAN.md` §3 L2. Rivisto `deserialize/standard_funcs.rs:288`:
  era `error!("could not cast ... field skipped")` — confermato che il record (`Equity`/`Bond`)
  viene comunque prodotto col campo vuoto, quindi non e' lavoro perso ma un caso di `warn!` come da
  tabella; corretto, doc-comment aggiornati. Aggiunto in `pdf_extract/tabularizer.rs` il `debug!`
  "table tabularized (rows x cols)" — l'esempio letterale della convenzione, sul vero tabularizer
  per-documento (distinto da `formats_repo/structured/tables.rs`, che legge configurazione). 61
  nuovi siti di log (12 `error!` di confine PyO3, 9 `warn!`, 17 `debug!`, 18 `trace!` sul percorso
  caldo per-riga/per-confronto che e' letteralmente l'esempio `trace!` della tabella). Comportamento
  segnalato non modificato: `PdfExtractAssetsStandard::call` tronca a `.min()` lunghezza fra cinque
  colonne (parita' voluta con la semantica di `zip` in Python). Nessun bug trovato. `cargo test --lib`
  -> **2511/0**, `cargo test` -> **65/0**.
- 2026-08-29 — **L2, area `output` (6/7) — sessione interrotta a meta', verificata dal diff.** La
  chiamata a `implementer` e' stata interrotta prima di restituire il rapporto finale, ma le
  modifiche gia' scritte sul disco erano complete e coerenti — verificate direttamente da `git diff`
  e da una nuova passata di test/clippy indipendente, non prese per buone da un rapporto mai
  arrivato. Aggiunto lo span mancante `write[<file>]` (`output / write[<file>]` del vocabolario
  `Activity` — lo span `output` stesso esisteva gia' da `cli`) in `routines/write.rs::write_csv_table`
  (la primitiva comune di scrittura CSV) piu' `write_additional_infos_yaml`/`compress_single_file`/
  `compress_directory`, ciascuno chiuso da `info!("file written")` (l'esempio letterale della
  convenzione). Un `warn!` nuovo per regola 1: `file_name_or_warn`, dove il riferimento degradava in
  silenzio a un nome archivio vuoto su un path non-UTF-8. `output/classes.rs`+7 sottomoduli e
  `files_schema.rs` confermati senza necessita' di log (le uniche modifiche presenti in quei file
  sono residui pre-esistenti e non correlati di F2/F3 — rename `#[serde(rename="name")]` di `Fund`,
  API test `from_iter`->`from_pairs` — verificati come precedenti a questa sessione, non toccati).
  Nessun bug trovato. `cargo build --lib` pulito; `cargo test --lib` -> **2511/0**; `cargo test` ->
  **65/0**.
- 2026-08-29 — **L2, area `commons` (7/7) — chiude L2.** Risolta la domanda aperta lasciata
  dall'entry `formats_repo`: i 5 file `python/` residui senza mappatura 1:1
  (`interfaces.rs`/`pipes.rs`/`api.rs`/`consts.rs`/`convert.rs`, ~1256 righe) ripiegati su questo
  ultimo passo come chiusura — `python/consts.rs` e' in realta' uno specchio diretto di
  `commons/consts.rs` per nome (stesso schema di `core`/`input`/`output`), gli altri quattro non
  mappano su una singola area ma questo evita di lasciarli scoperti a tempo indeterminato. 14
  `debug!` + 19 `error!` + 3 `trace!` sui 5 file `python/` (il grosso in `python/api.rs`, che prima
  non aveva **nessun** log nonostante importasse gia' l'infrastruttura `tracing`), 1 `debug!` in
  `commons/i18n.rs` (unico assorbimento genuino trovato in `commons` proprio: chiave di traduzione
  mancante, ricade sul `msg_id`). `commons/{consts,date,geometry,flag_expr}.rs` e
  `sets.rs`+3 sottomoduli lasciati intatti, verificato che ogni funzione propaga con `?` o va in
  panico su un'invarianza documentata gia' preservata dal riferimento (non un errore recuperabile
  assorbito in silenzio — la regola 1 non si applica). **Una citazione nel rapporto dell'agente e'
  risultata infondata**: citava una decisione di `critic` gia' registrata in questo stesso file per
  giustificare di non loggare `flag_expr::evaluate` — verificato con grep, quella voce non esiste da
  nessuna parte in questo `STATUS.md`. Il ragionamento tecnico sottostante (leaf condivisa, l'unico
  chiamante e' anch'esso una leaf) e' stato riverificato indipendentemente e regge comunque da solo;
  solo la citazione era falsa, segnalata qui come promemoria di verifica. **Scoperta strutturale
  vera, verificata**: `python/api.rs::py_run_job` apre il suo `tracing::subscriber::with_default`
  solo al proprio interno, e `core::tracing_setup::init` (che installa il subscriber globale) viene
  chiamato solo da `main.rs` (confermato via grep, nessun altro punto nel crate) — quindi ogni
  funzione di `python/api.rs` invocata fuori dallo scope di `py_run_job` (incluse le nuove chiamate
  di questa sessione) non ha nessun subscriber attivo quando il processo e' un semplice
  Python/pytest (ogni test a pagina singola di `freeports_dev`). Non corretto qui (e' una questione
  di wiring/tempistica del subscriber, non di log mancanti) — segnalato come probabile concern per
  L3. `cargo test --lib` -> **2511/0**, `cargo test` -> **65/0**, `cargo clippy` invariato.
- 2026-08-29 — **Bug ereditato segnalato durante L2 (area `cli`), non corretto per decisione
  dell'utente**: `config_locations/file.rs::find_config` ritorna `None` immediatamente se la
  directory corrente non e' leggibile, saltando del tutto la ricerca sui livelli utente/sistema
  anche se nessuno dei due dipende dalla cwd — uno scenario concreto: un `~/.config/freeports.yaml`
  valido verrebbe ignorato se la cwd del processo risultasse irraggiungibile (cancellata, smontata,
  NFS) nel momento della chiamata. Presentate tre opzioni (correggere ora, lasciare com'e', parametro
  opt-in); **risposta dell'utente: lasciare com'e'** — il `warn!` aggiunto durante lo sweep basta per
  ora come visibilita', comportamento invariato.
- 2026-08-29 — Il classificatore di auto mode ha bloccato due volte le operazioni di scrittura
  bulk su file di test (cancellazione+rigenerazione dei 228 JSON; riscrittura in-place dei 35
  `filter_data.json`): in entrambi i casi e' stata chiesta ed ottenuta conferma esplicita
  dall'utente prima di procedere.
- 2026-08-29 — **Q-L1 risposta: rigenerazione una tantum.** L'utente autorizza la rigenerazione dei
  31 `tests/formats/*/out/.log.csv` come parte di L1, sul modello di verifica gia' usato in F3
  (checksum sul contenuto equivalente, revisione del delta) invece della modalita' di compatibilita'
  col vecchio schema. L1 sblocca a cascata L2/L3.
- 2026-08-29 — **Q-L2 risposta: no** (come raccomandato dal piano). La sola colonna `Activity` non
  basta a generare una riga in `.log.csv`: serve almeno un campo fra
  `page`/`coord_ref_1`/`coord_ref_2`/`coord_1`/`coord_2`, per se' o ereditato dagli span. `.log.csv`
  resta il registro degli eventi localizzati, non un log completo.
- 2026-08-29 — **L1 chiusa.** Pipeline completa `implementation-planner` -> `critic` ->
  `test-writer` -> `implementer`, piano e revisione in `agent-memory/L1-implementation-plan.md`
  (incluso `§10` con le correzioni critic). Punti tecnici da ricordare oltre questo piano:
  - **Rottura di API pubblica**: `core::tracing_setup::init` cambia da `Result<(), _>` a
    `Result<CsvLogLayer, _>`. Unico consumatore reale: `main.rs`. `python/api.rs::py_run_job`
    aggiornato con la stessa precedenza d'errore di `main.rs` (l'errore del job vince su quello di
    `close()` se falliscono entrambi).
  - **Comportamento nuovo osservabile**: un fallimento di scrittura di `.log.csv` puo' ora
    terminare il processo CLI con errore (prima l'I/O era per-evento, silenzioso in caso di
    problemi). `main.rs` chiama `close()` **prima** del `tracing::error!` finale che segnala un
    `run::execute` fallito: quell'evento non puo' quindi mai finire nel CSV (nessun campo taggato
    oggi, ma trappola per il futuro se qualcuno ci aggiungesse un campo taggato — commento lasciato
    nel codice).
  - **`RowOrderKey` e' oggi `(page, sequence)`**, non ancora "documento, pagina, step, sequenza" di
    `PLAN.md` perche' non esistono ancora span `document`/`step` (saranno di L2/P1). Due trappole
    documentate nel codice per chi estende la chiave: (1) `derive(Ord)` confronta i campi
    nell'ordine di dichiarazione nello struct, quindi `document`/`step` vanno dichiarati **prima**
    di `page`, non appesi dopo `sequence`; (2) un futuro campo `document` dovrebbe essere un indice
    di job (`u64`), non l'id-documento come stringa, altrimenti l'ordinamento diventerebbe
    alfabetico invece che per ordine di arrivo del batch (comportamento attuale).
  - **Il meccanismo di accumulo-e-ordinamento non e' esercitato da nessun test end-to-end**: il
    confronto pytest del repo formati ordina entrambi i lati prima di confrontare (multiset, non
    sequenza), e i 4 fixture con dati erano gia' in ordine di pagina crescente prima della
    rigenerazione. **L'unica prova che l'ordinamento funzioni e' il sottomodulo unitario
    `row_ordering`** in `core::tracing_setup`. Una futura sessione P2 (dove l'ordine di arrivo
    diventa davvero non deterministico) non deve assumere che la suite di 259 test del repo
    formati coprirebbe una regressione di ordinamento: non lo farebbe.
  - **Race nota di `tracing-core` sulla cache d'interesse dei callsite** (gia' documentata una
    volta a M9 per un caso piu' stretto): `tracing_core::callsite::register_dispatch`, chiamato ad
    ogni `Dispatch::new` (quindi ad ogni `with_default`), ricalcola l'interesse di **tutti** i
    callsite gia' registrati nel processo, ANDando il risultato su **tutti** i dispatcher
    correntemente vivi — non solo quello che si sta installando. Un test con un livello statico
    (es. `LevelFilter::WARN` di `stderr_layer`) puo' quindi azzerare per sempre l'interesse di un
    callsite `info!`/`debug!` completamente estraneo, se e' vivo nell'istante sbagliato. Fix
    applicato: **un solo `static SERIAL: Mutex<()>` a livello di modulo**, condiviso da *tutti* i
    test di `core::tracing_setup` che installano un dispatcher (non solo quelli che condividono lo
    stesso callsite, come nella versione M9 piu' stretta) — verificato con 20+ run consecutivi
    dell'intero modulo, tutti verdi. Rischio residuo noto e non affrontato: il primo test di
    `global_init` installa un dispatcher **globale permanente** (`set_global_default`, mai
    rimosso per il resto del processo di test) che in teoria resta nell'insieme dei dispatcher vivi
    per ogni test successivo — non osservato in pratica, segnalato solo per visibilita'.
  - **Rigenerazione dei 31 `tests/formats/*/out/.log.csv`** (Q-L1): eseguita con uno script una
    tantum (non committato) che riproduce la chiamata di `PipelineTest.runtest` e copia solo il
    `.log.csv` prodotto. Precondizione verificata: **tutti e 31** i formati avevano *solo*
    `.log.csv` rosso prima della rigenerazione (`pytest tests/formats` -> 31 falliti su `.log.csv`,
    228 passati). Checksum SHA-256 su tutto `out/**` **tranne** i 31 `.log.csv` -> **identico**
    prima/dopo (277 file). Verifica finale: `pytest tests/formats` -> **259 passati, 0 falliti**.
  - **Scoperta non prevista dal piano, durante la revisione manuale del delta** (obbligatoria,
    fatta comunque nonostante l'esito positivo dei checksum): per i 4 fixture con righe dati
    (`DANSKEINVEST-EN24`, `CARNE-EN23`, `MEDIOLANUM-IT24.A`, `MEDIOLANUM-IT24.C`), oltre alla
    rimappatura di colonna il **contenuto dei messaggi e' cambiato**, e in un caso (`MEDIOLANUM-
    IT24.A`, pagina 611/ISHARES) due righe sono sparite del tutto — cosa che il piano dava per
    certo non sarebbe successa ("nessuna riga guadagna o perde contenuto in Page/Message").
    **Causa verificata, non ipotizzata**: il testo vecchio (es. `"Trying to cast to number but
    found '-365,138.81' - forcing cast"`, su tre righe distinte `Trying to cast`/`Error casting`/
    `Skipping field`) non compare in nessun punto della storia git del crate Rust (`git log -S`
    sul testo: zero risultati) — e' un residuo del **vecchio motore Python**
    (`freeports_core`/`freeports_analysis`, pre-riscrittura). Il testo nuovo (es. `"trying to cast
    to number but found a non-numeric shape - forcing cast"`) e' gia' presente, committato, in
    `cast.rs` a `HEAD` — non toccato da L1 ne' da questa sessione. Conclusione: questi 4 file erano
    fermi al motore Python e non sono mai stati toccati ne' verificati durante l'intera
    riscrittura Rust (M0-M10, F1, F2, F3), perche' l'invariante "`out/**` non si tocca" li ha
    sempre protetti fino a questa autorizzazione Q-L1. **Confermato esplicitamente dall'utente**
    (2026-08-29, "accetta e chiudi L1") dopo aver presentato la causa verificata: il contenuto
    nuovo riflette codice Rust gia' committato, indipendente da L1, e i dati finanziari
    (`investments.csv` ecc.) erano gia' validati corretti prima di questa sessione.

- 2026-08-30 — **L4, messa a punto del logging dopo la revisione dell'utente su L2.** Cinque cause
  misurate del rallentamento (~100x), non ipotizzate, con i numeri in
  `agent-memory/L4-logging-tuning-plan.md` §0. La piu' grave: **`CsvLogLayer` era installato senza
  filtro di livello**, e un layer senza filtro lascia il livello massimo globale del registry a
  `TRACE` — ogni `trace!` del crate veniva costruito e distribuito *sempre*, anche a `-q`. Unita'
  al fatto che `page` e' un campo **ereditato dallo span**, quindi `has_any_tagged_field()` e' vero
  per ogni evento dentro una pagina, il risultato era un `.log.csv` di **2,8 GB** su un solo job,
  accumulato in memoria prima di essere ordinato e scritto. Le altre quattro: `file_layer` cablato
  a `DEBUG` a prescindere dalla verbosita' e su `File` non bufferizzato;
  `FieldVisitor::record_debug` che formattava ogni campo di ogni evento *prima* di controllarne il
  nome, buttando via il risultato.
- 2026-08-30 — **L4: `EventLevelFilter`, la trappola dei filtri per-layer.** Mettere un semplice
  `LevelFilter` sui layer non basta e anzi rompe: un filtro per-layer **gate anche la creazione
  degli span**. A `WARN` gli `info_span!` del crate non venivano piu' aperti, e un `warn!` dentro
  una pagina perdeva il campo `page` ereditato — quindi niente colonna `Page` e nemmeno la riga nel
  `.log.csv`. Misurato durante l'implementazione: i 391 `warn!` reali di un job EURIZON-EN23
  producevano **zero** righe invece di 391. `EventLevelFilter` (in `tracing_setup.rs`) lascia
  passare **sempre** gli span e filtra solo gli eventi, con `max_level_hint` fermo a `SPAN_LEVEL`
  (`INFO`): i `debug!`/`trace!`, dove vivono i cicli caldi, restano spenti al callsite. Cinque test
  in `tests::csv_layer::event_level_filter` difendono le due meta' del contratto.
- 2026-08-30 — **L4: resa degli span `nome[valore]`, coerente fra le tre destinazioni.** Nuovo
  `SpanPathFormat` (un `FormatEvent`) piu' `SpanValueFields` (un `FormatFields`) per stderr e
  `freeports.log`, e `SpanLabel` memorizzato negli extension dello span per la colonna `Activity`:
  `run/job[EURIZON-EN23]/step[0]/page[353]/pipeline[investments]/text_filter/pipe[...]` invece di
  `run:job{format=EURIZON-EN23}:step{step=0}:page{page=353}:...`. E' la richiesta dell'utente
  ("class= e page= si potrebbero togliere") e insieme il vocabolario che `PLAN.md` §3 L1 aveva
  specificato ma che `activity_path` non produceva: prima stampava i soli nomi degli span. Un
  valore vuoto non produce parentesi (`pipeline`, non `pipeline[]`).
- 2026-08-30 — **L4: livello per destinazione.** stderr e `freeports.log` seguono `-v`/`-q`
  (prima il file era cablato a `DEBUG`); `.log.csv` segue la verbosita' ma **cappato a `DEBUG`**
  (`CSV_MAX_LEVEL`) sul percorso CLI, e fisso a **`WARN`** (`CSV_DEFAULT_LEVEL`) sul percorso
  Python, che non ha una verbosita' propria. La scelta del percorso Python non e' arbitraria: a
  `DEBUG` un singolo fixture di riferimento passava da 28 a 3047 righe, che non e' ne' rivedibile
  da un umano ne' un artefatto sensato da confrontare a ogni esecuzione dei test. `freeports.log`
  passa da `File` nudo a `BufWriter` condiviso, e `init` restituisce un `LogHandle` che chiude
  **entrambe** le destinazioni bufferizzate (prima ritornava il solo `CsvLogLayer`).
- 2026-08-30 — **L4: lo span `page` mancava in `Algorithm::classify_pages`.** Era l'unico buco
  della richiesta "la pagina esca anche in TextFilter e Deserialize": nella fase di esecuzione
  `page_span` avvolge gia' `bundle.apply`, quindi i tre segmenti lo ereditano, ma il ciclo su
  `doc.pages` della classificazione non ne apriva nessuno — ed e' la fase che produce la
  maggioranza degli eventi (22.800 su 33.086 nel log dell'utente). Tre righe.
- 2026-08-30 — **L4: potatura e contestualizzazione (richiesta esplicita dell'utente).** Rimossi i
  `trace!` per-elemento dei cicli caldi: `input/document/page_dict.rs` (uno per span/riga/blocco
  tipografico — da soli erano quasi tutto il `.log.csv` da 2,8 GB), `text_filter/matcher.rs` (300
  aziende per ogni frammento di testo, quasi tutte righe `matched=false`),
  `pdf_extract/select/pdf_line.rs` (una per riga per foglia di selezione),
  `tabularizer/coordinates.rs`. Rimossi i ~25 `debug!` "costruito da Python" di `python/*`, che
  dicevano solo "sono stato chiamato". I messaggi a conteggio sono diventati **contestuali** —
  regola dell'utente: *il testo trovato batte il numero di blocchi, perche' il testo si cerca con
  Ctrl-F dentro il PDF*. Nuovo `searchable_excerpt` in `core/pipeline/segment.rs`, e
  `log_segment_output` che emette la riga **solo** se c'e' almeno un risultato e un estratto non
  vuoto: `PdfExtractPageClassifyStandard` restituisce sempre un blocco vuoto anche quando la pagina
  non e' della sua class, e contarli produceva 11.259 righe identiche e prive di contenuto su un
  solo documento. Le classificazioni di pagina si loggano solo quando **riescono**, con il testo
  dell'intestazione riconosciuta. Effetto a `-vv`: da 20.700 a **7.194** righe, tutte ancorate a
  un testo cercabile.
- 2026-08-30 — **L4: rigenerazione dei `.log.csv` di riferimento, autorizzata dall'utente.**
  22 dei 31 confrontati sono cambiati; gli altri **277** file di `out/**` sono byte-identici
  (checksum SHA-256 prima/dopo, backup completo dei 308 file preso prima di scrivere). Le 46 righe
  che c'erano prima ci sono ancora tutte, invariate; le nuove sono tre `warn!` reali che prima non
  raggiungevano mai il CSV perche' non avevano una pagina nello scope: 1738 "no investment rows
  extracted - discarding the fund/currency blocks", 14 "expected text block not found near the
  matched company - row skipped", 1 "assets columns have mismatched lengths - extra entries
  dropped". **Da portare all'utente**: il primo domina il file (1738 righe su 1783) e vale la pena
  chiedersi se sia davvero un `warn!` o il caso normale di una pagina di continuazione — e' un
  livello deciso in L2, non da questa fase, quindi non e' stato toccato di iniziativa (politica
  "rust-migration-bugfix-policy").

- 2026-08-30 — **L4, secondo giro: `.log.csv` accanto agli output, non nella cwd.** Richiesta
  dell'utente. Il vincolo non e' banale: il CLI installa il subscriber in `main.rs` *prima* di
  risolvere la configurazione (la risoluzione stessa logga), quindi al momento dell'installazione
  non si sa ancora dove finiscono gli output. Soluzione: `CsvLogLayer::deferred()` — il layer nasce
  **senza destinazione**, accumula le righe in memoria (cosa che gia' faceva dall'L1 per il
  determinismo dell'ordinamento) e riceve il file solo con `LogHandle::set_csv_dir`, che
  `cli::run::execute` chiama non appena `resolve_configs` ritorna. Nessuna riga si perde, comprese
  quelle prodotte dalla risoluzione della configurazione. Se una corsa muore prima di risolvere,
  `close()` ripiega sulla cartella data a `init` (la cwd). La scelta della cartella e' una sola
  funzione condivisa, `cli::output::log_csv_dir`, usata anche da `python::api::py_run_job`, che
  prima duplicava la stessa logica: divergere li' vorrebbe dire che lo stesso job scrive il
  registro in due posti diversi a seconda di come e' stato lanciato.
- 2026-08-30 — **L4, secondo giro: `.log.csv` solo `warn` e `error`.** `CSV_MAX_LEVEL` da `DEBUG` a
  `WARN`. Sotto il tetto il file segue ancora `-q` (a `-q` resta solo `error`, a `-qq` tace). Due
  misure a sostegno del tetto, entrambe fatte in questa fase: senza alcun filtro il file arrivava a
  2,8 GB su un solo job, e anche cappato a `DEBUG` un fixture di riferimento passava da 28 a 3047
  righe.
- 2026-08-30 — **L4, secondo giro: resa della riga di log per destinazione.** stderr perde il
  timestamp e guadagna il colore (livello per severita', `Activity` in ciano, target in grigio
  scuro, campi in dim); `freeports.log` tiene il timestamp e non ha colori. La divisione e'
  deliberata: stderr si legge dal vivo, mentre la corsa e' davanti, e li' il timestamp e' rumore
  (parole dell'utente) mentre il colore e' cio' che rende la riga scandagliabile; `freeports.log`
  si legge dopo, spesso proprio per capire cosa ha impiegato tanto, e li' il timestamp e' il punto.
- 2026-08-30 — **L3 chiusa: `YamlLogLayer`.** Q-L3 risolta con l'opzione (b) raccomandata dal
  piano. Il campo `type` ipotizzato dal piano e' diventato `debug`: un `&dyn Error` non puo'
  dichiarare il proprio tipo concreto (`type_name_of_val` su un trait object risponde
  `dyn core::error::Error`, `Error::type_id` e' unstable), ma `{:?}` su un enum `thiserror` stampa
  gia' variante e campi, che e' strettamente di piu' del nome del tipo. Perche' il file non
  restasse vuoto e' servito uno **sweep di 53 siti** `error!`/`warn!` che interpolavano l'errore
  nel messaggio: ora agganciano anche `error = log_error(&e)`. Il messaggio non e' stato toccato
  (nessuna deriva dei fixture, stderr e `.log.csv` restano leggibili da un umano) e in cambio
  `EventFieldVisitor` **non stampa piu' il campo `error`** sulle destinazioni testuali, che
  altrimenti direbbero la stessa cosa due volte sulla stessa riga. Un solo sito e' rimasto fuori,
  `python::utils::pdf_extract::value_error`, perche' e' generico su `Display` e viene chiamato
  anche con `String`.
- 2026-08-30 — **L3: `record_span_metadata`/`merge_span_fields` estratte e condivise.** Trovato
  scrivendo i test di `YamlLogLayer`: `SpanLabel` e `CapturedFields` erano scritti **solo** da
  `CsvLogLayer::on_new_span`, quindi un registry col solo layer YAML produceva record con
  `activity: page` invece di `page[12]` e senza nessuna coordinata. In produzione non si vedeva
  (i due layer ci sono entrambi), ma era una dipendenza silenziosa fra layer che dovrebbero essere
  indipendenti. Ora le due funzioni sono libere, idempotenti, e chiamate da entrambi.
- 2026-08-30 — **Il `warn!` "no investment rows extracted" e' stato declassato a `debug!`
  dall'utente** (modifica sua a `formats_utils/text_filter/standard_funcs.rs`, rilevata alle 09:09
  dal timestamp del file): e' la risposta al punto che questa fase aveva lasciato aperto. I 31
  `.log.csv` di riferimento sono stati rigenerati una seconda volta di conseguenza. Bilancio finale
  rispetto all'inizio della sessione: **8 file su 31** cambiati (non 22), **zero** degli altri 277
  file di `out/**` toccati (checksum SHA-256), le 46 righe preesistenti tutte intatte, e **15 righe
  nuove** — 14 "expected text block not found near the matched company - row skipped" e 1 "assets
  columns have mismatched lengths - extra entries dropped", due `warn!` reali che prima non
  raggiungevano mai il CSV per mancanza di una pagina nello scope.
- 2026-08-30 — **Colonne d'ancoraggio usate davvero.** Tre `warn!` portavano il loro ancoraggio
  testuale come campo libero (`company`, `field`) invece che nelle colonne `First/Second coord ref`,
  che esistono esattamente per quello: un testo con cui ritrovare la riga nel PDF. Come campo
  libero restavano leggibili solo su stderr e sparivano dalle colonne del `.log.csv`. Corretti in
  `text_filter/standard_funcs.rs` (`coord_ref_1 = company`) e `core/promisable.rs`
  (`coord_ref_2 = field`).

- 2026-08-30 — **L5: `freeports.log` diventa `freeports.log.jsonl`, JSON Lines.** L'utente lasciava
  scegliere fra JSON e YAML. Scelto **JSON Lines**, cioe' un oggetto per riga, per due ragioni che
  vengono entrambe dal volume che il file vede a `-vvv` (3,5 MB per un solo job da 1140 pagine):
  *streaming* — ogni record raggiunge il writer bufferizzato quando accade, quindi niente si
  accumula in memoria e il log di un processo morto a meta' resta leggibile, cosa che un array JSON
  o un documento YAML, che vanno chiusi in fondo, non possono garantire — e *indipendenza della
  riga*, che tiene funzionanti `grep` e `jq`. Il timestamp non e' sparito: e' migrato dal prefisso
  della riga al campo `time`, con lo stesso identico formato di prima, ottenuto riusando il
  `SystemTime` di `tracing_subscriber` invece di aggiungere una dipendenza di date.

- 2026-08-30 — **L5: `LogRecord`/`build_record` condivisi fra i due log strutturati.** La richiesta
  "nel log su file, quando l'evento e' legato a un `Err`, mettine la deserializzazione" si risolve
  meglio condividendo la forma del record che duplicandola: `YamlRecord` -> `LogRecord`,
  `YamlVisitor` -> `RecordVisitor`, e il corpo di `YamlLogLayer::on_event` estratto in
  `build_record`, che ora serve entrambi i layer. Cosi' i due file non possono divergere su *cosa*
  dice un record — differiscono solo su quali eventi prendono e su come li serializzano. E' la
  stessa lezione dell'estrazione di `record_span_metadata`/`merge_span_fields` fatta in L3.

- 2026-08-30 — **L5: `.log.csv` non compare piu' nella cartella corrente.** Non era una svista di
  L4 ma il suo fallback deliberato: `LogHandle::close` ripiegava sulla cartella data a `init` (la
  cwd, per la CLI) quando `set_csv_dir` non era mai stato chiamato. Ci si arriva a **ogni corsa che
  fallisce prima della risoluzione della configurazione**, cioe' prima della riga di
  `cli::run::execute` che fissa la destinazione. Riprodotto — non ipotizzato — con
  `freeports -i /nonexistent/x.pdf -f NOPE -o ./outdir`, che lasciava in cwd un `.log.csv` di 80
  byte, esattamente il file trovato in `packages/freeports/`. Il fallback e' stato tolto:
  `CsvLogLayer::discard()` butta le righe orfane invece di creare un file dove non deve stare. Le
  righe non sono davvero perse — sono eventi che hanno gia' raggiunto stderr e il log su file.

- 2026-08-30 — **L5: la seconda causa del `.log.csv` nella cwd era la suite di test.** Trovata
  perche' il file ricompariva anche dopo aver tolto il fallback: `cargo test` esegue i binari di
  test con **cwd nella radice del package**, non nella cartella da cui lo si lancia, e
  `cli::run::tests::error_propagation::an_unknown_format_surfaces_as_cli_error_job` e' l'unico test
  del modulo che arriva oltre la risoluzione della configurazione — quindi l'unico in cui `execute`
  chiama davvero `set_csv_dir`. Non passava `--out`, cosi' `out_path` prendeva il suo default (la
  cwd assoluta, `partial_config::defaults`) e ogni `cargo test` lasciava un `.log.csv` di 80 byte
  in `packages/freeports/`. E' quasi certamente l'origine del file che l'utente ha segnalato:
  l'orario corrispondeva a una corsa dei test, non a una del binario. Corretto aggiungendo `--out`
  su una sottocartella della tempdir; verificato che una suite completa non lasci piu' niente.

- 2026-08-30 — **L5: il `target` esce da stderr ed entra nei record.** Il percorso del modulo
  (`freeports::formats_utils::text_filter::standard_funcs`) e' il token piu' lungo della riga e
  quasi mai cio' che si cerca leggendo una corsa dal vivo. Tolto da stderr su richiesta
  dell'utente; e' un campo di ogni record di `freeports.log.jsonl` e `.freeports.log.yaml`, dove si
  puo' filtrare. Con `freeports.log` diventato strutturato, `SpanPathFormat` ha ora una sola
  destinazione: sono spariti sia il flag `timestamp` sia il flag `target`, e con loro `GuardWriter`
  e l'impl `MakeWriter for SharedFileWriter`, che non serviva piu' a nessuno.

- 2026-08-30 — **L5: quattro colori nel percorso degli span, non uno.** Nomi degli span in ciano,
  valori fra parentesi in **magenta chiaro**, `/` e `[`/`]` in grigio scuro, campi dell'evento in
  dim. Tinte diverse e non sfumature della stessa: la richiesta e' distinguere segmenti e parametri
  *a colpo d'occhio*, e la tinta e' cio' che lo fa. Il grigio sulla punteggiatura e' la meta' meno
  ovvia della scelta — e' pura struttura, quindi e' l'unica parte che deve arretrare.

- 2026-08-30 — **L5: `python::utils::pdf_extract::value_error` sdoppiata.** Era l'ultimo sito
  `error!`/`warn!` del crate senza `error = log_error(..)`, e la nota in L3 diceva perche': la
  firma era generica su `Display` per servire l'unico chiamante che passa una `String`. Ora quel
  caso ha la sua funzione (`value_error_msg`) e `value_error` chiede un vero
  `std::error::Error + 'static`, cosi' i suoi 9 chiamanti serializzano l'errore. Verificato con uno
  scan che apre ogni invocazione di `tracing::error!`/`warn!` (anche multilinea) e cerca un errore
  interpolato senza campo strutturato: **zero** siti rimasti.

- 2026-08-30 — **L5: `.freeports.log.yaml` e' ora un sottoinsieme di `freeports.log.jsonl`.**
  Segnalato all'utente, non deciso qui. A `-vvv` il `.jsonl` contiene gia' tutti i record
  `warn`/`error` con l'errore serializzato, piu' tutto il resto: il YAML resta perche' e' piccolo e
  leggibile da un umano, ma la sovrapposizione e' reale e va decisa da chi ha chiesto L3 ieri.

- 2026-08-30 — **L5, non fatto di iniziativa: ANSI anche quando stderr non e' un terminale.**
  Comportamento di L4, non introdotto da L5, ma con quattro colori invece di uno un `2>file`
  raccoglie sensibilmente piu' sequenze d'escape. Un `std::io::IsTerminal` su stderr lo
  risolverebbe ed e' la convenzione comune, ma cambia cio' che l'utente vede quando redirige
  (niente piu' colori in un file letto con `less -R`), quindi va deciso con lui.

- 2026-08-30 — **L5: i 93 s di `pytest tests/formats` registrati per L4 non sono riproducibili.**
  Misurati 236 s oggi. Verificato che non sia una regressione di L5 ricompilando l'estensione dallo
  stato pre-L5 e rieseguendo la suite: **236,3 s** senza L5 contro 236,0 s e 236,9 s con L5 —
  differenza nulla, e coerente col fatto che `py_run_job` monta il solo `CsvLogLayer`, che L5 non
  tocca. Il numero di ieri resta non spiegato: ambientale, non attribuibile ne' a L4 ne' a L5. Chi
  riprende quest'area usi 236 s come riferimento, non 93.

- 2026-08-30 — **Q-P1 risposta: processi figli con IPC dei risultati.** Il padre ri-esegue il
  proprio eseguibile una volta per job (`std::process::Command`, mai `fork`); ogni figlio serializza
  il proprio `Vec<DocumentOutcome>` in un file temporaneo e il padre aggrega e scrive **una volta
  sola**, come oggi. Scartate le due alternative offerte: "ogni figlio scrive il proprio output in
  una sottocartella" (eviterebbe il lavoro serde ma cambierebbe la semantica di output della
  modalita' batch, e con essa i file di riferimento del repo formati) e "solo thread" (il GIL
  riserializzerebbe proprio il caricamento PyMuPDF, cioe' il 35-75% misurato da P0, lasciando a P1
  un guadagno che P2 copre gia').
- 2026-08-30 — **Q-P2 risposta: basta l'equivalenza semantica**, il determinismo byte-per-byte non
  e' un vincolo. L'invariante §6.2 di `PLAN.md` e' stata riscritta di conseguenza. Attuazione scelta
  in P1, e segnalata all'utente: la risposta *permette* l'ordine non deterministico, non lo impone,
  quindi dove ordinare costa zero si ordina lo stesso. I risultati dei job vanno in slot indicizzati
  per posizione, cosi' l'output aggregato resta **byte-identico** a quello sequenziale anche con
  N > 1 e i 259 test del repo formati restano confrontabili per checksum; il margine concesso si
  spende solo sull'unione dei tre file di log, dove un ordinamento globale costerebbe un merge vero.
- 2026-08-30 — **Q-P0 rimandata**, non risposta: l'utente ha scritto "procedi con P1".
  L'ottimizzazione interna di `TextFilterInvestmentsStandard` (il 30-54% del tempo totale di un job)
  resta disponibile come passo a se' dopo P1/P2, con il vantaggio di non avere alcun rischio di
  non-determinismo.
- 2026-08-30 — **La premessa di `PLAN.md` §4 P1 era sbagliata**, scoperto leggendo il codice prima
  di iniziare. Il piano diceva che ogni job "scrive i propri CSV: nessuna memoria condivisa";
  `cli::run::execute` invece concatena i `DocumentOutcome` di tutti i job e chiama
  `output::write_results` **una volta sola**, con i parametri di scrittura della **prima**
  configurazione risolta. E' la ragione per cui P1 richiede un IPC dei risultati e non dei semplici
  processi indipendenti. `PLAN.md` §4 P1 corretto sul posto.

- 2026-08-30 — **P1 chiusa.** Otto passi, ciascuno con i test scritti prima dell'implementazione.
  Le scelte che meritano di essere ricordate:
  - **Due file JSON, non una pipe.** Lo stdout di un figlio non e' un canale pulito: PyMuPDF e i
    pipe Python d'autore ci scrivono quando vogliono. Un file per direzione non ha quel problema,
    non ha il limite di dimensione di una pipe, e sopravvive al figlio abbastanza da essere letto
    dopo che e' uscito.
  - **Un job fallito non e' un figlio fallito.** Un errore di dominio produce un
    `WorkerReport::Failed` e il figlio esce **con 0**: l'errore e' nel payload. Il codice d'uscita
    non-zero resta ai fallimenti di protocollo, che il padre riconosce dal referto mancante.
    Confonderli renderebbe indistinguibile "il PDF non esiste" da "il figlio e' morto di segnale".
  - **Il messaggio d'errore attraversa il confine verbatim.** L'errore tipizzato no — un enum non
    si ricostruisce da una stringa — ma `ErrorRecord` porta forma `Debug`, forma `Display` e catena
    di `source()`, e la forma `Display` e' esattamente cio' che il caso sequenziale stampa. Un test
    d'integrazione confronta i due stderr: stesso messaggio a `-j 1` e a `-j 2`.
  - **Il primo fallimento in ordine di job, non il primo arrivato.** E' cio' che rende l'errore
    riportato lo stesso che il `for` sequenziale avrebbe propagato, comunque siano andate le corse.
  - **Differenza osservabile da non nascondere**: il `for` sequenziale si ferma al primo job che
    fallisce, quindi i job successivi non partono mai. Con un pool possono essere gia' partiti. Il
    padre li lascia finire e poi riporta il primo fallimento in ordine, senza scrivere nulla —
    stesso errore, stesso codice d'uscita — ma un job successivo a quello fallito **puo' aver
    prodotto effetti collaterali** che in sequenziale non avrebbe prodotto: in pratica il solo
    `save_pdf`, cioe' un PDF scaricato e salvato.
  - **`n_workers` trova il suo primo consumatore.** Esisteva da sempre (config, `--workers`/`-j`,
    `FREEPORTS_N_WORKERS`) ed era inutilizzato; ora decide quanti job alla volta, limitato dal
    numero di job. Default 1, quindi **chi non chiede nulla percorre esattamente il codice di
    prima**. Lo schema `parallelism` completo resta lavoro di P5.
- 2026-08-30 — **Tre scostamenti dal piano di P1**, decisi durante l'implementazione:
  1. `tempfile` e' solo una dev-dependency, quindi l'area di lavoro dei figli segue la convenzione
     gia' in uso in `cli::job` per i PDF temporanei (`std::env::temp_dir()` +
     `freeports-jobs-<pid>`), con la cancellazione in `Drop` invece che a fine funzione — cosi'
     l'area sparisce anche quando la corsa esce per un errore. Nessuna dipendenza nuova.
  2. `ErrorRecord` (L3) e' diventato `pub` e `Deserialize` invece di essere duplicato in
     `cli::worker`: serviva la stessa forma per far viaggiare un errore, e riscriverla avrebbe
     duplicato anche la guardia sui cicli di `source()`.
  3. **L'unione dei log passa dalla memoria, non dai file.** Riversare i figli direttamente nei
     file del padre non puo' funzionare: `.log.csv` e `.freeports.log.yaml` vengono *troncati*
     dalla scrittura del padre, che avviene solo alla chiusura, e il `.jsonl` ha un `BufWriter` con
     un proprio offset — un append esterno verrebbe sovrascritto. I log dei figli sono quindi
     assorbiti in memoria (`LogHandle::absorb_worker_logs`) mentre l'area di lavoro esiste ancora,
     e riversati in `close()` **dopo** cio' che il padre ha scritto. Coerente col fatto che le
     righe del `.log.csv` del padre erano gia' bufferizzate in memoria fino alla chiusura.
- 2026-08-30 — **Attuazione della risposta a Q-P2, e perche' e' piu' stretta della risposta.**
  L'equivalenza semantica *permette* l'ordine non deterministico, non lo impone. I risultati dei
  job vanno in slot indicizzati, che non costano nulla: l'output aggregato resta **byte-identico**
  a quello sequenziale anche con N figli, ed e' un test d'integrazione (`-j 1` contro `-j 4`), non
  una speranza. Il margine concesso si spende in due punti soli: lo **stderr** dei figli, ereditato
  e quindi interlacciato fra job (ogni riga resta intera e porta il proprio percorso di span), e
  l'**unione dei tre file di log**, raggruppata per job invece che ordinata globalmente — una riga
  riletta da un CSV non ha piu' la `RowOrderKey` su cui ordinarla, e inventargliene una la
  metterebbe in un ordine che non ha nulla a che vedere con quando e' successa.
- 2026-08-30 — **Misura del guadagno di P1.** I quattro PDF di P0 non sono piu' nel workspace; la
  misura e' stata rifatta sui due EURIZON-EN23 disponibili, come batch di 2 job: **13,34 s a `-j 1`
  contro 7,08 s a `-j 2`, cioe' 1,88x**, tre ripetizioni stabili entro l'1%. Coerente con P0: il
  caricamento PyMuPDF e' il 35-75% del tempo e i processi sono l'unico livello che lo parallelizza
  davvero — un tetto ideale di 2,00x su due job, raggiunto al 94%. **Caveat onesto sulla misura**:
  entrambe le corse falliscono allo stesso modo *dopo* aver eseguito i due job, nell'aggregazione
  finale del padre (`funds_assets: duplicate Fund ID|Date`), perche' EURIZON-EN23.A e .B sono due
  meta' dello stesso report annuale e i loro fondi collidono quando finiscono in un unico output.
  E' una proprieta' di questa coppia di documenti, non di P1: il lavoro cronometrato e' identico
  nelle due corse, ed e' tutto il lavoro dei job. Da rifare su documenti indipendenti quando ce ne
  saranno di nuovo quattro sul disco, insieme al `p0_profile` che `PLAN.md` §4 P0 chiede di
  rieseguire dopo P1..P4.

- 2026-08-30 — **P2 chiusa.** Le decisioni non previste da `PLAN.md`, tutte motivate in
  `agent-memory/P2-implementation-plan.md`:
  - **D-P2-1, pool rayon dedicato.** `core::parallelism` costruisce un `rayon::ThreadPool` proprio,
    pigramente, in un `OnceLock`. Non si chiama `build_global`: il pool globale appartiene a chi
    incorpora il crate (l'example `p0_profile`, un consumatore Python, un binario di terze parti), e
    sequestrarlo sarebbe una decisione presa a nome suo. Se rayon non riesce a costruirlo, il motore
    torna sul percorso sequenziale invece di fallire — il parallelismo e' un'ottimizzazione, non un
    requisito di correttezza.
  - **D-P2-2, il parallelismo e' un parametro e non uno stato dell'`Algorithm`.** Le firme storiche
    (`apply`, `apply_multidocument`, `classify_pages`, `classify_pages_multidocument`) restano e
    valgono **sequenziale**; accanto nascono le varianti `*_with(..., Parallelism)`. Nessuno dei
    2.600 test esistenti ha cambiato comportamento, e il ramo `pages == 1` e' letteralmente il
    codice di prima.
  - **D-P2-3, degradazione rilevata per bundle e non per formato.** `PLAN.md` §4 chiedeva di
    rilevare i pipe Python e degradare a sequenziale; farlo per *formato* sarebbe stato sbagliato, e
    la misura lo conferma: EURIZON-EN23 ha la classificazione in Python e gli step in Rust,
    MEDIOLANUM-ES24.B ha un `deserialize` d'autore nella pipeline degli investimenti, UBS-EN23 sta
    nella cartella `unstructured/` ma usa **solo** pipe standard. Degradando per formato, due dei
    tre avrebbero perso il guadagno. Meccanismo: `scales_with_threads()` sui tre trait dei pipe,
    `true` di default, `false` sui tre `Py*Pipe`.
  - **D-P2-4, l'errore riportato e' quello della pagina di numero piu' basso.** I risultati per
    pagina si raccolgono in un `Vec<Result<..>>` indicizzato e si riducono **dopo**, in ordine.
    Costa l'esecuzione delle pagine successive a una che fallisce — il ciclo sequenziale si fermava
    alla prima — ma rende il messaggio d'errore identico a quello del caso sequenziale senza
    dipendere da quale reduce interno usi rayon. Un errore fatale a meta' documento e' raro; un
    messaggio d'errore che cambia da una corsa all'altra no.
  - **D-P2-5, gli span si riagganciano a mano.** Ogni closure cattura la `Span::current()` del
    chiamante e la rientra prima di aprire il proprio `page`. Senza, la colonna `Activity` del
    `.log.csv` si svuoterebbe proprio dove serve. E' l'unico requisito di P2 non verificabile con un
    test unitario — serve un subscriber **globale**, perche' quello di `with_default` e'
    thread-local — quindi ha un file d'integrazione tutto suo,
    `tests/algorithm_parallel_pages.rs`.
  - **D-P2-6, quanto parallelismo in attesa di P5.** `pages = auto`, **diviso** per il numero di job
    che P1 esegue insieme, cosi' che un batch `-j 4` su 20 thread non ne apra 80. Il valore viaggia
    verso il processo figlio in un campo nuovo di `WorkerRequest` (`page_workers`) e non in una
    variabile d'ambiente, per la stessa ragione per cui ci viaggia la configurazione risolta: il
    figlio non deve ri-derivare nulla che il padre abbia gia' deciso.
  - **`python::api::run_job` resta sequenziale**, per due ragioni indipendenti e ognuna sufficiente:
    il suo subscriber e' installato con `with_default`, il cui scope e' thread-local (i thread rayon
    non lo vedrebbero, e ogni evento delle pagine distribuite sparirebbe dal `.log.csv` che quella
    funzione esiste per produrre); e un `#[pyfunction]` gira con il GIL gia' preso dal chiamante,
    quindi un pipe d'autore su un thread rayon aspetterebbe un GIL che il chiamante non rilascia
    finche' non ha finito di aspettare i thread — uno stallo, non un rallentamento. La degradazione
    di `scales_with_threads` lo eviterebbe gia', ma non e' il genere di cosa da lasciare appesa a
    una sola difesa.
  - **Il punto 1 di `PLAN.md` §4 P2 (`Page::raw` e il GIL nel `Drop`) non si e' presentato**, e la
    verifica e' stata fatta leggendo il codice invece di assumerlo: nel ciclo parallelo le pagine
    sono **prestate** (`&Page` dentro `ScheduledPage`), i `Document` restano di proprieta' di
    `cli::job::run_impl` e muoiono sul thread chiamante, uno alla volta, esattamente come prima.
  - **`examples/p0_profile.rs` accetta ora `--pages N`** (default 1, cioe' il comportamento
    documentato in `agent-memory/P0-profile.md` invariato parola per parola): e' con quel flag che
    il confronto sequenziale/parallelo di sopra e' stato misurato. Attenzione leggendone le tabelle
    a `--pages N > 1`: i tempi per percorso di span si sommano **per thread**, quindi il tempo
    inclusivo di uno span puo' superare il tempo di parete. Il numero confrontabile e'
    `apply_multidocument`, misurato con un `Instant` e quindi tempo di parete.

- 2026-08-30 — **Bug preesistente scoperto durante le verifiche di P2, non causato da P2 e non
  corretto: il binario `freeports` esce con SIGSEGV sul formato CARNE-EN23.** Tutti gli output sono
  scritti correttamente prima del crash (le nove CSV sono byte-identiche fra corsa sequenziale e
  parallela), e il codice d'uscita 139 arriva **dopo** la fine del lavoro. Il backtrace del core
  dump lo colloca fuori dal crate: `exit()` -> distruttore statico C++ `mupdf::internal_state` ->
  `fz_drop_context` -> `fz_flush_warnings` -> callback SWIG di PyMuPDF -> `libpython3.14`, cioe'
  MuPDF che svuota le proprie warning richiamando Python a interprete gia' finalizzato. Nessun frame
  di `freeports` e nessun frame di rayon nella catena. Riprodotto tre volte su tre, **identico sul
  percorso sequenziale** (`taskset -c 0`, che porta `available_parallelism` a 1 e quindi non tocca
  nemmeno il pool), e -- prova decisiva -- **identico anche sul binario costruito da `HEAD`**
  (`96b7fad6`, cioe' prima di P1, di P2 e di tutto il lavoro di oggi), compilato in un worktree
  temporaneo apposta e poi rimosso. Gli altri quattro formati provati (EURIZON-EN23, AMUNDI-EN24, UBS-EN23,
  DANSKEINVEST-EN24) escono con 0. Non toccato per la politica di
  `rust-migration-bugfix-policy`: si segnala e si aspetta conferma. Q-P2b.

## Note aperte lasciate da F2

- **Rottura di API Rust pubblica**, contenuta ma reale: `FlatPromiseMap` e' re-esportato sia da
  `src/api.rs:70` sia (per una svista del confine dichiarato in `lib.rs`, gia' presente prima di F2:
  vedi `agent-memory/F2-implementation-plan.md` §9-C6) da `freeports::core::promise_resolution` via
  `pub mod core` in `src/lib.rs:12`. Cambiano le firme di `get`/`iter` (`Option<&[BlockValue]>`,
  `(&String, &[BlockValue])` invece di `Option<&BlockValue>`, `(&String, &BlockValue)`) e sparisce
  `impl FromIterator for FlatPromiseMap`. Unico consumatore reale verificato con grep su tutto il
  workspace: `tests/algorithm_end_to_end.rs`, gia' aggiornato. Nessun altro crate o repo usa questi
  tipi.
- **Lacuna di test nota, non colmata in F2** (dal piano, §5.7): non esiste un test che eserciti il
  percorso Python -> `PromiseEntries` con un pipe che restituisce una lista di dict per depositare
  piu' contributi sullo stesso id (il pattern descritto in "Decisioni prese" 2026-08-29, C5). Non
  aggiunto qui perche' richiederebbe un interprete Python attivo in `py_pipe.rs` (che oggi non ha
  `mod tests`) e ricadrebbe fra i 6 test gia' rossi per venv/`fitz` mancante quando il venv non e'
  attivo. Se si riprende quest'area, e' il punto da cui iniziare.
- Doc-comment di `promise_resolution.rs`/`promisable.rs` corretti nei fatti ma **restano in
  italiano** per scelta di perimetro (F1/F2 non traducono, la traduzione e' D2 — vedi `PLAN.md`
  §2 D2 e Q-F1).

- 2026-08-30 — **P0 chiusa. Strumento: un example di cargo, non codice di produzione.** Il profilo
  poteva essere ottenuto in due modi: aggiungere un layer di profilazione a `core::tracing_setup`
  dietro un flag, oppure metterlo fuori dal crate compilato. Scelto il secondo
  (`packages/freeports/examples/p0_profile.rs`): gli `info_span!` che L2 ha installato ovunque
  sono gia' tutta la strumentazione che serve, quindi il profilo si legge senza toccare una riga
  di produzione, senza rischio di deriva sui fixture e senza far crescere il binario. L'example
  resta in albero perche' la stessa misura va rifatta dopo P1..P4 per verificare il guadagno; non
  e' compilato da `cargo build` e non entra nel wheel. Due accorgimenti per rendere il numero
  confrontabile con una corsa vera: il filtro replica `EventLevelFilter` alla verbosita' di
  default (span sempre, eventi a `WARN` — profilare a `-vvv` avrebbe misurato il logging), e la
  sequenza eseguita e' quella di `cli::job::run_impl`, non una sua approssimazione. Verificato:
  binario `release` 0,72/2,58/17,75/21,40 s contro profilo 0,70/2,55/17,66/21,30 s sugli stessi
  quattro job — l'overhead sta sotto l'avvio del processo.
- 2026-08-30 — **Quattro documenti invece dei tre chiesti dal piano.** I tre citati da `PLAN.md`
  §4 P0 (MEDIOLANUM-ES24.B 29 pagine, UBS-EN23 222, AMUNDI-EN24 1.824) lasciavano scoperta una
  casella che e' proprio quella su cui il piano fondava il rischio §9.3: un documento **grande e
  con pipe Python d'autore** (AMUNDI-EN24 e' interamente `structured`, cioe' Rust puro). Aggiunto
  EURIZON-EN23 variante 1 (1.140 pagine, classificazione scritta in Python), ed e' il documento
  che ha smentito il rischio.
- 2026-08-30 — **Il rischio `PLAN.md` §9.3 era mal puntato, riscritto.** L'ipotesi era che il GIL
  azzerasse il guadagno sui formati `unstructured`. La misura dice il contrario: i pipe Python
  d'autore costano millisecondi, con **una** eccezione (la classificazione di EURIZON-EN23:
  1,08 s, il 6,1% del job). Il GIL fa male in un punto solo, **PyMuPDF nel caricamento**, che e'
  il 35-75% del tempo e non e' codice d'autore — non lo si evita scegliendo un livello di
  specifica diverso. La conclusione operativa del rischio resta valida (su documento singolo P2 ha
  un tetto di 1,5-2,9x, il grosso e' P1), ma per un motivo diverso da quello scritto.
- 2026-08-30 — **P4 chiusa senza implementazione.** Il piano condizionava esplicitamente
  l'unica parallelizzazione dei blocchi di `deserialize` a "solo se P0 lo mostra". P0 non lo
  mostra: `deserialize` costa 22-27 ms su job da 17-21 secondi, sotto lo 0,2% del totale su tutti
  e quattro i documenti. Segnato ❌ e non ⬜ perche' e' una decisione presa da una misura, non un
  lavoro rimandato. Conseguenza su P5: `deserialize_blocks_threshold` esce dallo schema
  `parallelism` invece di restare un'opzione che non fa niente.
- 2026-08-30 — **P2 riceve un ordine interno che il piano non aveva.** I due punti di
  `core::algorithm` non pesano uguale: il ciclo delle pagine di uno step contiene
  `TextFilterInvestmentsStandard` (l'85-96% del lavoro del motore, Rust puro, nessun GIL) e va
  parallelizzato per primo; `classify_pages` vale da 1:8,5 a 1:157 di meno, e dove varrebbe
  qualcosa e' scritta in Python, quindi il GIL la riserializza comunque.
- 2026-08-30 — **Q-P0, domanda nuova che P0 apre e che va all'utente.** La misura ha trovato una
  cosa che il piano non prevedeva: un **solo** pipe, `TextFilterInvestmentsStandard`, e' il 30-54%
  del tempo *totale* di un job su tutti e quattro i documenti (14-20 ms a pagina di investimenti,
  contro 0,01 ms di un pipe di classificazione standard). E' Rust mono-thread e deterministico:
  ottimizzarlo internamente — indicizzare le societa' bersaglio invece di scorrerle, ridurre le
  compilazioni di regex per chiamata — potrebbe valere quanto tutta la fase P, senza alcun rischio
  di non-determinismo. Da notare che il numero e' un **minimo**: l'input db di test ha 76
  societa', uno di produzione ne ha molte di piu' e il costo di quel pipe cresce con quel numero.
  Non fatto di iniziativa: e' un passo che il piano non contiene, e la scelta e' dell'utente.

- 2026-08-31 — **P3 chiusa senza implementazione**, seconda volta che una misura chiude un passo
  invece di aprirlo (la prima e' P4). Il piano raccomandava di implementarla "con default
  disattivato", ma quella raccomandazione e' **anteriore a P0** e non teneva conto di P2: oggi le
  pagine di un gruppo di page class prendono gia' tutti i thread della macchina, quindi P3
  annide**rebbe** rayon su 1-2 unita' di lavoro senza avere thread liberi a cui darle. Decisa senza
  chiedere perche' la richiesta originale lo prevede esplicitamente ("non penso abbia senso
  parallelizzare tutto... valuta una strategia tenendo in conto della mole di lavoro che ogni layer
  deve fare") e perche' la misura che la chiude e' gia' agli atti (`agent-memory/P0-profile.md`
  §5). **Se l'utente la rivuole, va riaperta come passo a se'**: non e' una dimenticanza.
- 2026-08-31 — **P5, D-P5-1: `n_workers` e' il default globale, non un sinonimo di `pages`.**
  `PLAN.md` §4 P5 conteneva due frasi in tensione fra loro — "«`n_workers` ... diventa il default
  globale. Sopra ci va una sezione dedicata, con override per livello»" e "«`--workers/-j N` senza
  altro imposta `pages = N` e lascia il resto al default»". Vinta la prima, che e' anche la piu'
  utile: `-j N` imposta entrambi i livelli, quindi `-j 1` significa di nuovo *esattamente
  sequenziale* (l'invariante di §6, prima raggiungibile solo scrivendo due valori) e `-j N` fa
  qualcosa di sensato anche su un documento solo, dove `jobs` non ha nulla da parallelizzare. La
  seconda frase era anteriore a P1, quando il livello job non era ancora configurabile. Chi vuole
  un livello solo ha `--jobs`/`--pages`.
- 2026-08-31 — **P5, D-P5-4: il default in batch cambia comportamento, di proposito.** Prima
  `n_workers` valeva `1` e un batch girava sequenziale se nessuno chiedeva altro; ora entrambi i
  livelli valgono `auto`, come lo schema di `PLAN.md` §4 P5 prescrive e come chiede la richiesta
  originale ("dei default sensati"). Guadagno **2,36x** su due job grandi. Il prezzo e' la
  **memoria**: N job concorrenti caricano N PDF interi, e il picco misurato passa da **783 MB** a
  **~1,2 GB** con due soli job. Su una macchina con molti core e poca RAM un batch grande puo'
  quindi costare parecchio; il tetto si abbassa con `parallelism.jobs` o `--jobs`. **Segnalato
  all'utente**: se il default prudente e' preferibile, e' una riga da cambiare
  (`partial_config::defaults`).
- 2026-08-31 — **P5, D-P5-3: un `pages` esplicito non si divide fra i job concorrenti.** In `auto`
  il budget di core si divide (invariante introdotta da P2); un numero scritto dall'utente si
  onora com'e', anche quando `jobs x pages` supera i core. In quel caso `resolve_parallelism`
  emette un `warn!` con i tre numeri: `PLAN.md` §2 principio 4 vieta gli override silenziosi, non
  le configurazioni scomode.
- 2026-08-31 — **Difetto di P1 scoperto da P5 e corretto: `serde_json` senza `float_roundtrip`.**
  Il referto che un job worker rimanda al padre e' JSON, e la *lettura* di un `f64` da JSON in
  `serde_json` **non e' esatta** senza la feature `float_roundtrip`: un valore la cui
  rappresentazione decimale piu' corta non e' quella scritta torna indietro spostato di un ULP.
  Effetto osservato: `investments_add_infos.yaml` con `interest_rate: 0.02925` da una corsa
  sequenziale e `0.029249999999999998` da una corsa in processi — nessun errore, nessun fallimento,
  solo due output diversi per lo stesso input. Isolato per livello: e' il livello **job** (P1), non
  quello pagina (P2), che resta byte-identico. Corretto abilitando la feature in `Cargo.toml`, con
  un test di regressione al confine IPC che fallisce davvero senza di essa
  (`cli::worker::tests::report_round_trip::a_float_survives_the_report_bit_for_bit`, verificato
  togliendo e rimettendo la feature). Corretto invece che segnalato soltanto perche' P5 rende quel
  percorso il **default**: lasciarlo avrebbe significato consegnare un default che viola
  l'invariante 2 di questo documento. Da rileggere insieme alla chiusura di P1, che dichiarava
  `out/**` byte-identico — e lo era, perche' quei confronti passavano dal percorso sequenziale.

## Invarianti (non negoziabili senza l'utente)

1. `tests/formats/*/out/**` non si tocca — unica eccezione possibile `out/.log.csv`, solo con
   autorizzazione esplicita (Q-L1).
2. Con `parallelism` a 1 l'output e' identico byte per byte a quello di oggi; con N e' identico a
   quello con 1. *(Verificato end-to-end alla chiusura di P5, sulle quattro combinazioni dei due
   livelli piu' il default: `tests/cli_worker_processes.rs::parallelism_options`, e a mano su due
   report reali. E' l'invariante che ha fatto emergere il difetto `float_roundtrip` di P1.)*
3. Nessuna regressione: 2.474 unitari + 63 d'integrazione + 259 del repo formati.
4. Test Rust raggruppati per argomento in sottomoduli, mai lista piatta.
5. Modifiche al codice del repo formati: si propongono, non si applicano.
6. Bug ereditati: correzione alla radice, ma chiedendo prima, offrendo l'opzione "parametro opt-in
   con il vecchio comportamento come default".
