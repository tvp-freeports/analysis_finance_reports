# `freeports` — piano: parallelizzazione, logging, documentazione, correzioni

Documento di progetto per il lavoro che segue la riscrittura in Rust. Sorgente dei requisiti:
`packages/richieste.txt` (4 richieste dell'utente, 2026-08-24).

Il piano precedente — la riscrittura vera e propria, milestone M0..M10, tutte chiuse — non e'
piu' in albero ma resta recuperabile da git:

```bash
git show 13284baa:packages/freeports/PLAN.md
git show 13284baa:packages/freeports/STATUS.md
```

I suoi **principi architetturali (§2)**, lo **stile dei test (§10)** e le **decisioni prese
(§12/§13)** restano validi e vincolanti anche qui: questo documento aggiunge, non sostituisce.

---

## 0. Punto di partenza (verificato, non assunto)

| Fatto | Misura |
|---|---|
| Sorgenti del crate | 116 file `.rs`, 49.042 righe |
| Test | 2.474 unitari + 63 d'integrazione, verdi |
| Repo formati | `pytest tests/formats`: 259 passati / 0 falliti (baseline motore Python: 252/7) |
| Righe di doc-comment | 6.103, di cui ~2.961 in italiano |
| Siti di logging | **19** in tutto il crate, in 10 file su 116 |
| `span!` esistenti | **3** (`page`, `field`, la coppia societa' in `deserialize`) |
| `n_workers` | esiste in `FreeportsConfig` e in `--workers/-j`, **non e' usato da nessuna parte** |
| Fixture a pagina singola (repo formati) | 76 pagine x 3 file = 228 JSON, 175 dei quali con tag di modulo legacy |
| Report di test reali | 21 PDF, mediana **288 pagine**, media 480, massimo **1.824** |

Questi ultimi due numeri sono il dato che orienta tutta la §3 (parallelizzazione): il documento
tipico non ha "qualche decina" di pagine, ne ha **centinaia**.

---

## 1. Le quattro aree e il loro ordine

| Fase | Richiesta | Contenuto | Dipende da |
|---|---|---|---|
| **F** | 4 | Correzioni: lingua inglese, ambiguita' della multimap delle promise, rigenerazione delle fixture a pagina singola | — |
| **L** | 2 | Logging: nuovo schema `.log.csv` con lo span logico, strumentazione capillare, `.freeports.log.yaml` | F (baseline pulita) |
| **P** | 1 | Parallelizzazione a piu' livelli, configurabile | L |
| **D** | 3 | Documentazione: doc-comment, whitepaper, strategia sphinx/mdbook/rustdoc | F, L, P |

**Perche' questo ordine, e non quello della richiesta.**

1. **F prima di tutto** perche' la rigenerazione delle fixture a pagina singola stabilisce la
   baseline verde su cui misurare qualunque altra modifica, e perche' la rinomina degli
   identificatori italiani tocca gli stessi file che L e D riscriveranno: farla dopo vorrebbe dire
   toccarli due volte.
2. **L prima di P**, e questo e' il punto non ovvio. Gli span di `tracing` **non attraversano da
   soli** un confine di thread: dentro una closure `rayon` `Span::current()` e' vuoto, e va
   riagganciato esplicitamente. Se si parallelizza prima e si strumenta dopo, la strumentazione va
   scritta due volte (una versione sequenziale e una che si porta dietro lo span). In piu' la
   gerarchia di span che la richiesta 2 chiede (`run/pipeline_investment/pdf_extract/...`) e'
   esattamente la mappa dei punti dove la richiesta 1 vuole parallelizzare: descriverla prima
   significa progettare la parallelizzazione su una struttura gia' esplicita.
3. **P prima di D** perche' il whitepaper deve descrivere il modello di esecuzione definitivo, non
   quello intermedio.
4. **D per ultima**, e assorbe la traduzione dei doc-comment (vedi Q-F1): sono le stesse righe.

---

## 2. Fase F — correzioni (richiesta 4)

### F1. Tutto in inglese

**Stato reale.** L'inventario e' fatto: ~187 nomi di funzione/test contengono parole italiane
(concentrati in `core/classes.rs`, `core/match_fund.rs`, `core/normalization.rs`,
`core/promise_resolution.rs`, `core/promisable.rs`, `core/classes/value.rs`), e ~30 file hanno
variabili locali italiane (`appiattiti`, `contributi`, `candidati`, `unico`, `in_corso`,
`valori`, `attesa`, `riga`, ...). Il file piu' colpito e' `core/promise_resolution.rs`.

**Perimetro F1**: identificatori (funzioni, variabili locali, campi privati, nomi di test e di
sottomodulo di test). **Non** i doc-comment — quelli sono D2, che li riscrive comunque.

**Metodo.** Rinomine meccaniche, un modulo per volta, `cargo test` dopo ciascuno. Nessuna
rinomina di API pubblica senza segnalarla: se un nome italiano e' esposto in `api.rs` o in
`python.rs`, cambia anche il contratto verso il repo formati e ricade sotto la regola "cambiamenti
al repo formati si propongono, non si applicano" (vedi §7).

### F2. Multimap delle promise: `BlockValue::List` e' ambiguo (l'utente ha ragione)

**Il dubbio dell'utente e' confermato dal codice, non e' teorico.** In
`core/promise_resolution.rs`:

- `PromiseMap::flatten` (righe ~145-152) riduce N contributi per lo stesso id a **un solo**
  `BlockValue`: 1 contributo -> quel contributo; N>1 contributi -> `BlockValue::List(contributi)`;
  0 contributi -> l'id sparisce.
- `FlatPromiseMap::fulfill` (righe ~193-205) legge quel valore e, **se e' una `List`, la tratta
  come l'elenco dei candidati**: promessa `multiple` -> tutti; promessa normale -> `candidati.last()`.

Quindi un id con **un solo** contributo che e' *davvero* una lista (`BlockValue::List([a, b])`)
e' bit-per-bit indistinguibile da **due** contributi scalari `a` e `b`. Una promessa normale su
quel valore restituisce `b` invece della lista intera; una promessa `multiple` restituisce
`[a, b]` invece di `[[a, b]]`. Non e' un caso di scuola: `BlockValue::List` e' una variante
legittima che un pipe puo' depositare.

E' un **bug latente**, non una scelta di design: nessun commento del modulo lo rivendica, e il
riferimento Python aveva la stessa forma (`mapping.get(id, [value])`) per ragioni di comodita',
non per disegno.

**Ricade sotto la politica dei bug ereditati** (`agent-memory`, "rust-migration-bugfix-policy"):
si porta all'utente con piu' opzioni, non si sceglie da soli. Le tre da mettere sul tavolo:

- **(a) Separare il contenitore dal contributo.** `FlatPromiseMap` non memorizza un `BlockValue`
  ma un `Contributions(Vec<BlockValue>)` (o `FlatPromiseMap { entries: BTreeMap<String,
  Vec<BlockValue>> }`, che e' la stessa cosa senza tipo nuovo): un contributo resta un contributo,
  una lista resta una lista. `fulfill` diventa banale e totale. E' la correzione alla radice; costa
  il cambio di una firma pubblica e la revisione dei test di `flatten`.
- **(b) Variante nuova `BlockValue::Multi(Vec<BlockValue>)`**, distinta da `List`, usata solo
  dall'appiattimento. Meno invasiva sulle firme, ma aggiunge una variante a un enum che attraversa
  serializzazione, `Ord`, `PartialEq` e la API Python: costo nascosto alto.
- **(c) Parametro opt-in** (la forma che l'utente ha gia' scelto per `keep_sign` in M4): `flatten`
  prende un flag che, se attivo, conserva la distinzione; i chiamanti esistenti passano il valore
  che riproduce il comportamento attuale. Minimo rischio di regressione, ma lascia il default
  sbagliato.

**Raccomandazione: (a)**, perche' e' l'unica che elimina l'ambiguita' invece di aggirarla, e
perche' `FlatPromiseMap` non e' un tipo che il repo formati costruisce a mano (lo produce il
motore) — la superficie da adeguare e' interna. Da confermare: **Q-F2**.

**Chiusa 2026-08-29: opzione (a).** Implementata secondo il piano dettagliato in
`agent-memory/F2-implementation-plan.md` (con la revisione critica in quel documento §9 e le
decisioni finali in §10 su tre sotto-domande emerse in corso d'opera: dove si scarta un contributo
`Null`, come reagire alla rottura silenziosa di `FromIterator`, e se la perdita apparente di
capacita' sulla superficie Python fosse reale — non lo era). Dettagli e numeri di verifica in
`STATUS.md` riga F2 e "Decisioni prese durante l'implementazione".

### F3. Fixture a pagina singola rigenerate, `_LEGACY_MODULES` rimosso

Il codice incriminato e' esattamente questo, in
`packages/freeports_dev/src/freeports_dev/serialization.py`:

```python
_LEGACY_MODULES = {
    "freeports._internals.output.classes_schema": "freeports.output",
    "freeports._internals.commons.consts":        "freeports.consts",
    "freeports._native":                          "freeports.core",
    "freeports._internals.core.classes":          "freeports.core",
}
```

Rimappa in lettura i tag delle fixture vecchie. Fa passare i test raccontando che il layout
`_native`/`_internals` esiste ancora: e' proprio la retrocompatibilita' fuorviante che la
richiesta chiede di togliere, ed e' incoerente con la regola gia' registrata "niente `_native`
ne' `_internals`".

**Piano.**

1. Script di rigenerazione che cammina `tests/formats/*/pages/<page_class>/<N>-*.json`, ne ricava
   `(formato, page_class, pagina)` dal percorso, e per ciascuno chiama
   `freeports-dev make-tests --format F --page N --page-class C` con il `report.pdf` del formato.
   76 pagine, 26 formati.
2. **Precondizione per formato**: si rigenerano le fixture di un formato **solo se il suo test
   d'integrazione e' verde** (`out/*.csv` invariati). Rigenerare una fixture significa dichiarare
   corretto cio' che il motore produce oggi: senza il test d'integrazione verde a fare da
   contro-prova, si rischia di cristallizzare un errore. Un formato con l'integrazione rossa si
   ferma e si segnala.
3. Rimozione di `_LEGACY_MODULES` e di `_resolve_module`; `_resolve_class` importa il modulo del
   tag e basta.
4. Verifica: nessun `_internals`/`_native` residuo nei 228 JSON (`grep`), suite completa verde.

**Vincolo assoluto**: `tests/formats/*/out/**` — i CSV di riferimento e il loro `.log.csv` — **non
si toccano in questa fase**. La richiesta autorizza la rigenerazione dei *test a pagina singola*,
non degli output d'integrazione, che restano la specifica eseguibile del motore. (La fase L li
mette pero' in discussione: vedi **Q-L1**, che e' bloccante.)

**Chiusa 2026-08-29.** Rigenerati i 228 JSON via `freeports-dev make-tests` (script che cammina
`pages/<page_class>/<N>-*.json`, cancella i tre file e li rifà, precondizione "integrazione verde"
soddisfatta da tutti i 26 formati fin dall'inizio); riscritti a mano i 35 `filter_data.json` con la
stessa mappa dello shim, perché `make-tests` li legge ma non li riscrive mai; rimossi
`_LEGACY_MODULES`/`_resolve_module`. La rigenerazione ha fatto emergere due bug reali (non ipotesi
di piano): `_promise_tag` scriveva `str(p)` invece di `p.id`, e `Fund` (unica fra le sette entità)
aveva il campo serde `n_name` disallineato dal costruttore pubblico `name` — quest'ultimo corretto
alla radice in `output/classes/fund.rs` con `#[serde(rename = "name")]`, con conferma esplicita
dell'utente prima di toccare il crate. Dettagli, numeri di verifica e cronologia in `STATUS.md` riga
F3 e "Decisioni prese durante l'implementazione".

---

## 3. Fase L — logging (richiesta 2)

### L1. Nuovo schema di `.log.csv`: la colonna dello span logico

**Header attuale** (`core::tracing_setup::CSV_HEADER`):

```
Page,Matched Company,Company,Field name,Row,Column,Message
```

**Header proposto**:

```
Page,Activity,First coord ref,Second coord ref,First coord,Second coord,Message
```

Mappatura richiesta -> implementazione:

| Colonna vecchia | Colonna nuova | Campo `tracing` | Nota |
|---|---|---|---|
| `Page` | `Page` | `page` | invariata |
| — | `Activity` | *(nessuno: calcolata)* | percorso degli span attivi, dal piu' esterno al piu' interno, unito con `/` |
| `Matched Company` | `First coord ref` | `coord_ref_1` | oggi alimentata da `company` |
| `Company` | *(eliminata)* | — | l'utente: "non serve piu'" |
| `Field name` | `Second coord ref` | `coord_ref_2` | oggi alimentata da `field` |
| `Row` | `First coord` | `coord_1` | il valore include l'unita': `row 12` |
| `Column` | `Second coord` | `coord_2` | idem: `col 3` |
| `Message` | `Message` | `message` | invariata |

**Semantica generalizzata** (e' il punto della richiesta): le due coordinate sono il **punto della
pagina** che ha fatto scattare l'evento, con unita' di misura dipendente dal contesto — oggi riga
e colonna di una tabella, domani un'ascissa in punti PDF o un indice di riga di testo. I due *ref*
sono ancoraggi **testuali** alla stessa posizione, non identificatori univoci: servono a un umano
per ritrovare il punto in un viewer PDF (e' il motivo per cui "Matched Company" funzionava bene).
Chi emette l'evento formatta il valore con la sua unita'; il layer non interpreta, stampa.

**`Activity` — la gerarchia degli span.** Vocabolario proposto, dall'esterno all'interno:

```
run / job[<config>] / document[<id>] / classify
                                     / step[<n>] / page[<n>] / class[<page_class>]
                                                  / pipeline[<nome>] / pdf_extract   / pipe[<nome>]
                                                                     / text_filter   / pipe[<nome>]
                                                                     / deserialize   / pipe[<nome>]
                                     / output / write[<file>]
load / formats_repo[<path>] / format[<nome>]
```

reso in colonna come `run/document[AMUNDI-EN24]/step[0]/page[44]/pipeline[investments]/deserialize/pipe[deserialize_investment]`.
Il nome dello span dice **cosa** stava facendo il codice; il modulo sorgente (`target` di
`tracing`) resta disponibile e va nel file `freeports.log`, non nel CSV.

**Regola di selezione delle righe**: invariata nella forma (una riga solo se l'evento, per se' o
per eredita' dagli span, porta almeno uno dei campi taggati) ma i campi da controllare diventano
`page`/`coord_ref_1`/`coord_ref_2`/`coord_1`/`coord_2`. Da decidere se `Activity`, che ora c'e'
sempre, debba da sola bastare a produrre una riga (**Q-L2**): se bastasse, `.log.csv` diventerebbe
un log completo e non piu' il registro degli eventi *localizzati*, che e' il suo scopo. La
raccomandazione e' **no**: `Activity` arricchisce la riga, non la giustifica.

**Determinismo (vincolo che lega L a P).** `.log.csv` e' confrontato da fixture: l'ordine delle
righe deve essere riproducibile. Con la fase P gli eventi nascono su thread diversi e l'ordine di
arrivo al layer non e' piu' quello logico. Il layer deve quindi **accumulare** le righe con una
chiave d'ordine (documento, pagina, indice step, sequenza) e scriverle ordinate alla chiusura,
invece di scriverle in streaming. E' una modifica di `CsvLogLayer` da fare **in L1**, prima di P,
anche se il motivo si manifesta solo dopo.

**Chiusa 2026-08-29.** Implementata secondo il piano dettagliato in
`agent-memory/L1-implementation-plan.md` (con la revisione critica in quel documento §10, sei
osservazioni tutte applicate prima di `test-writer`). Q-L1 risposta: rigenerazione una tantum dei
31 `tests/formats/*/out/.log.csv`, verificata con checksum SHA-256 sul resto di `out/**` (identico)
e revisione manuale del delta. Q-L2 risposta: no, `Activity` da sola non basta a generare una riga.
Durante la revisione manuale e' emersa una scoperta non prevista dal piano (4 dei 31 fixture con
dati portavano contenuto residuo del vecchio motore Python, mai toccato durante l'intera
riscrittura Rust): causa verificata e confermata dall'utente, dettagli in `STATUS.md` riga L1 e
"Decisioni prese durante l'implementazione". Numeri di verifica finali: `cargo test --lib` ->
**2511 passati, 0 falliti**; `cargo test` -> **65 d'integrazione passati, 0 falliti**; `pytest
tests/formats` -> **259 passati, 0 falliti**.

### L2. Strumentazione capillare, file per file

Oggi ci sono 19 log in tutto. La richiesta e' "cospargere tutto il codice di tutti i log che
possono servire, file per file, di tutti i livelli che servono": e' un lavoro di sweep su 116
file, e va fatto con una **convenzione scritta prima**, altrimenti diventa rumore.

Convenzione proposta per livello:

| Livello | Cosa ci va | Esempi |
|---|---|---|
| `error!` | il lavoro richiesto non e' stato prodotto | pipe fallito in modo non recuperabile, config non risolvibile, repo formati non caricabile |
| `warn!` | il lavoro prosegue ma qualcosa e' andato perso | pagina saltata, cast fallito e campo scartato, promessa non risolta, colonna assente |
| `info!` | i passaggi che un utente vuole vedere senza chiedere | documento caricato (N pagine), formato riconosciuto, step iniziato/finito, file scritto |
| `debug!` | i passaggi interni utili a chi sviluppa un formato | blocchi prodotti da un pipe (conteggio), page class assegnate, promesse depositate, tabella tabularizzata (righe x colonne) |
| `trace!` | il dettaglio che serve solo in un debug attivo | il contenuto dei blocchi, le selezioni valutate riga per riga, i confronti di matching |

Regole trasversali:
- **Ogni funzione che puo' fallire e assorbe l'errore** deve loggarlo prima di assorbirlo.
- **Nessun log dentro un ciclo caldo a livello superiore a `trace!`** (il tabularizer gira su
  migliaia di righe).
- **Gli span si aprono nei punti di orchestrazione** (`algorithm`, `pipeline`, `segment`, `job`,
  `run`, `output`), non dentro i pipe: un pipe eredita lo span di chi lo chiama.
- I campi delle coordinate si mettono **sugli span**, non sui singoli eventi, come gia' si fa oggi
  per `field` e la coppia societa'.
- **Messaggi di log, nomi di span e nomi di campo: in inglese**, coerentemente con F1/D2 (regola
  resa esplicita qui il 2026-08-29 dopo che l'area `cli` ha trovato un residuo italiano preesistente
  in `core/algorithm.rs`, `"pagina saltata"` — da correggere quando tocca all'area `core`).

Ordine di sweep per area (ognuna e' un'unita' di lavoro autonoma, con i suoi test):
`cli` -> `input` -> `formats_repo` -> `core` (algorithm/pipeline/schedule) -> `formats_utils`
-> `output` -> `commons`.

**Nota di perimetro (2026-08-29, richiesta esplicita dell'utente): `src/python/` non e' una delle
sette aree sopra, ma e' il layer di binding PyO3 che le specchia una per una** (`python/core.rs`,
`python/input.rs`, `python/output.rs` rispecchiano le aree omonime; `python/utils/{pdf_extract,
text_filter,deserialize}.rs` e `python/standard_funcs/{pdf_extract,text_filter,deserialize}.rs`
rispecchiano `formats_utils/*/standard_funcs.rs` e le funzioni "utils" dello stesso albero).
Essendo un ottavo albero con nome diverso dalle sette aree elencate, e' il punto piu' facile da
dimenticare nello sweep. Regola: **quando si strumenta un'area che ha uno specchio in `python/`,
lo specchio si strumenta nella stessa sessione**, non in un passaggio a parte — in particolare
`python/utils/*` e `python/standard_funcs/*` vanno fatti insieme a `formats_utils` (dove i pipe
author-facing chiamano dentro Rust: e' un punto di confine, quindi candidato naturale per
`debug!`/`warn!` sugli argomenti ricevuti e sugli errori di conversione PyO3). `cli` non ha
specchio in `python/` (non e' esposto a Python) — nessuna azione aggiuntiva per l'area corrente.

**Chiusa 2026-08-29.** Sweep completato su tutte e sette le aree (`cli` -> `input` ->
`formats_repo` -> `core` -> `formats_utils` -> `output` -> `commons`) piu' ogni specchio
`python/` (inclusi i cinque file residui — `interfaces.rs`/`pipes.rs`/`api.rs`/`consts.rs`/
`convert.rs` — senza una mappatura 1:1 su una singola area, ripiegati sull'ultimo passo
`commons` come chiusura, invece di lasciarli scoperti o inventare un'ottava area ricorrente).
Aggiunti gli span mancanti dal vocabolario `Activity` (§3 L1): `formats_repo[<path>]`/
`format[<nome>]` (in `formats_repo.rs::Algorithm::load`), `classify`/`step[<n>]`/
`class[<page_class>]`/`pipeline[<nome>]`/`pdf_extract`/`text_filter`/`deserialize`/`pipe[<nome>]`
(in `core/algorithm.rs` e `core/pipeline/segment.rs`), `write[<file>]` (in
`output/routines/write.rs`). Corretto il residuo italiano `"pagina saltata"` -> `"page skipped"`
(area `core`, come previsto) e altri due mislivellamenti trovati durante la revisione
(`unstructured/py_pipe.rs`: `info!` -> `warn!`; `deserialize/standard_funcs.rs`: `error!` ->
`warn!`). Verificato ad ogni passo, senza regressioni: `cargo test --lib` 2511/2511, `cargo test`
65 d'integrazione, `cargo clippy` invariato (6 warning pre-esistenti, mai aumentati). Nessun bug
di comportamento corretto silenziosamente; un bug gia' noto (`find_config`'s cwd-unreadable
fallthrough) segnalato e lasciato com'e' per decisione esplicita dell'utente. Due note strutturali
lasciate aperte per una sessione futura (non bloccanti per L2, dettagli in `STATUS.md`): lo span
`document[<id>]` non avvolge `classify`/`step`/`page` nel caso multi-documento; `python/api.rs`
non ha un subscriber `tracing` attivo fuori dallo scope di `py_run_job`, quindi molti dei nuovi
log in quel file sono no-op nei processi Python/pytest che non passano da `main.rs`.

### L4. Messa a punto del logging dopo L2 (revisione dell'utente)

Non era nel piano originale: nasce dalla revisione dell'utente su L2 chiusa (2026-08-29) — troppi
log, forma ripetitiva (`class{class=investments} page{page=353}`), programma **~100 volte piu'
lento** anche a sola verbosita' `warn`, messaggi che contano invece di contestualizzare, e la
pagina assente negli eventi di `TextFilter`/`Deserialize`.

Diagnosi misurata, decisioni dell'utente e passi in
`agent-memory/L4-logging-tuning-plan.md`. In sintesi, le quattro linee di intervento:

1. **Prestazioni.** La causa principale non era il volume dei log ma un difetto di installazione:
   `CsvLogLayer` era montato **senza filtro di livello**, e un layer senza filtro tiene il livello
   massimo globale del registry a `TRACE`. Correzione: `EventLevelFilter`, che lascia passare
   sempre gli **span** (altrimenti un `warn!` perde la pagina che eredita dallo span) e filtra solo
   gli **eventi**, con `max_level_hint` fermo a `SPAN_LEVEL`. Piu': livello per destinazione legato
   a `-v`/`-q`, `freeports.log` bufferizzato, `FieldVisitor` che non formatta piu' i campi che
   scarta.
2. **Forma.** `SpanPathFormat`/`SpanValueFields`/`SpanLabel`: la stessa resa `nome[valore]` in
   stderr, in `freeports.log` e nella colonna `Activity` del `.log.csv` — che finalmente rispetta
   il vocabolario di §3 L1 invece di stampare i soli nomi degli span.
3. **Quantita' e contenuto.** Potatura dei `trace!` per-elemento nei cicli caldi e dei `debug!`
   "costruito da Python", e riscrittura dei messaggi a conteggio in messaggi **contestuali**:
   *il testo trovato batte il numero dei blocchi, perche' il testo si cerca con Ctrl-F nel PDF*.
   Regola operativa che ne discende, da applicare anche a L3 e a ogni log futuro: **una riga di log
   si emette solo se porta qualcosa con cui ancorarla al documento**.
4. **La pagina nei tre segmenti.** Unico buco reale: `Algorithm::classify_pages` non apriva lo span
   `page`. Nella fase di esecuzione i tre segmenti lo ereditavano gia'.

**Chiusa 2026-08-30.** Stesso job (EURIZON-EN23.A, 1140 pagine, verbosita' di default): **13,0
secondi** contro i **19 minuti** di partenza; `.log.csv` da **2,8 GB** a **609 righe**;
`pytest tests/formats` da 388 s a **93 s**. `cargo test --lib` 2518/2518, `cargo test` 65
d'integrazione, `pytest tests/formats` 259/259 dopo la rigenerazione autorizzata di 22 `.log.csv`
di riferimento (gli altri 277 file di `out/**` byte-identici, provato con checksum). Due punti
lasciati all'utente, non decisi qui: il livello del `warn!` "no investment rows extracted" (1738
righe su 1783 nei fixture rigenerati) e il fatto che la chiave `verbosity` del file di
configurazione **non piloti** il logging (vedi sotto).

**Secondo giro, 2026-08-30** (richieste successive dell'utente, tutte chiuse):

- **`.log.csv` va nella cartella di `out`, non nella cwd.** Il vincolo non e' banale: il CLI
  installa il subscriber *prima* di risolvere la configurazione (la risoluzione stessa logga), e
  fino a quel momento non sa dove andranno gli output. `CsvLogLayer::deferred()` nasce senza
  destinazione, accumula le righe in memoria come gia' faceva dall'L1, e riceve il file da
  `LogHandle::set_csv_dir` appena `resolve_configs` ritorna. La cartella la calcola una sola
  funzione condivisa con il percorso Python, `cli::output::log_csv_dir`.
- **`.log.csv` solo `warn` e `error`** (`CSV_MAX_LEVEL` da `DEBUG` a `WARN`).
- **Resa della riga per destinazione**: stderr senza timestamp e con livello, `Activity` e target
  colorati; `freeports.log` con timestamp e senza colori. (Superata da L5: il log su file non e'
  piu' testo, e stderr non porta piu' il target.)
- Tre `warn!` portavano il loro ancoraggio testuale come campo libero invece che nelle colonne
  `First/Second coord ref`: corretti.

**Nota, non corretta in questa fase.** `main.rs` installa il subscriber usando i soli conteggi di
`-v`/`-q` della riga di comando, *prima* di risolvere la configurazione: la chiave `verbosity` di
`~/.config/freeports.yaml` (che l'utente ha valorizzata a `trace`) attraversa tutta la catena
`file`/`env`/`cmd` di `partial_config.rs` e finisce in `FreeportsConfig` senza influenzare nessuna
delle tre destinazioni di log. Sistemarlo richiede un `tracing_subscriber::reload` applicato dopo
la risoluzione della config — e' un cambiamento di struttura di `main.rs`, non un ritocco, quindi
va deciso con l'utente.

### L3. `.freeports.log.yaml` a verbosita' massima

Alla verbosita' massima (`-vvv`, `Verbosity::Trace`) si genera anche un file YAML con la
**serializzazione degli errori**. Da progettare come un quarto layer, `YamlLogLayer`, accanto ai
tre esistenti (stderr, `freeports.log`, `.log.csv`).

Forma proposta del record (un documento YAML, lista di errori):

```yaml
- activity: run/document[AMUNDI-EN24]/step[0]/page[44]/pipeline[investments]/deserialize
  level: WARN
  target: freeports::formats_utils::deserialize::cast
  message: "Error casting, skipping field: ..."
  coords: { first_ref: NORDEA, second_ref: market_value, first: row 12, second: col 3 }
  error:
    type: CastError::NotANumber
    display: "cannot cast 'n/a' to f64"
    source:
      - "invalid float literal"
```

Punto di progetto da risolvere (**Q-L3**): gli enum d'errore del crate sono `thiserror`, **non
`Serialize`**. Due strade: (a) derivare `Serialize` su tutti gli enum d'errore — invasivo,
~25 enum, e vincola la loro forma; (b) serializzare un **record strutturale** (nome del tipo via
`std::any::type_name`, `Display`, catena di `source()`), che non tocca gli enum e funziona anche
per errori di terze parti. Raccomandazione: **(b)**, con (a) eventualmente dopo, solo per gli
enum di cui serva davvero il dettaglio dei campi.

Da decidere anche: solo eventi `error!`/`warn!` o tutti; percorso del file (accanto a `.log.csv`,
quindi nella cartella di output); flag dedicato `--log-yaml` oppure implicito in `-vvv`.

Nota tecnica da verificare in fase di implementazione: `serde_yaml` 0.9 (gia' in `Cargo.toml`) e'
**non piu' mantenuto** a monte. Se il file YAML diventa un artefatto di prodotto conviene valutare
un sostituto mantenuto prima di costruirci sopra.

**Chiusa 2026-08-30.** `YamlLogLayer`, quarto layer accanto ai tre esistenti. Le tre decisioni
che il piano lasciava aperte sono state prese dall'utente: **solo `warn!`/`error!`** (e' il log
degli errori, non un secondo trace); **nella cartella corrente**, non in quella di output, perche'
e' un artefatto diagnostico e non un prodotto della corsa — al contrario di `.log.csv`, che nello
stesso giro si e' spostato *dentro* `out`; **implicito in `-vvv`**, nessun flag dedicato.

**Q-L3 risposta: opzione (b)**, il record strutturale, come raccomandato. Con una correzione al
piano: il campo `type` non e' ottenibile. Un `&dyn Error` non sa dichiarare il proprio tipo
concreto (`type_name_of_val` su un trait object risponde `dyn core::error::Error`, `Error::type_id`
e' unstable), quindi al suo posto c'e' `debug`, il `{:?}` dell'errore — che su un enum `thiserror`
stampa variante e campi (`AlgorithmLoad(UnknownFormat { format: "NOPE", known: 27 })`) ed e'
strettamente piu' informativo del nome del tipo.

Il punto non ovvio: perche' il file si popoli davvero serviva uno sweep di **53 siti**
`error!`/`warn!` che l'errore lo interpolavano nel messaggio, dove la catena di `source()` va
perduta. Ora agganciano anche `error = log_error(&e)`. Il messaggio non e' stato toccato — nessuna
deriva dei fixture, e stderr/`.log.csv` restano leggibili da un umano — e in cambio la resa
testuale **non stampa piu' il campo `error`**, che altrimenti direbbe la stessa cosa due volte
sulla stessa riga.

`serde_yaml` 0.9 e' rimasto: e' gia' dipendenza e gia' scrive `investments_add_infos.yaml`. La nota
del piano sulla sua manutenzione a monte resta valida come debito, non come blocco.

### L5. Log su file strutturato, stderr leggibile a colpo d'occhio

Non era nel piano: nasce dalla revisione dell'utente su L3/L4 chiuse (2026-08-30). Cinque
richieste, tutte chiuse lo stesso giorno; passi e diagnosi in
`agent-memory/L5-structured-log-plan.md`.

1. **`freeports.log` diventa `freeports.log.jsonl`**, strutturato: un oggetto JSON per riga.
   L'utente lasciava scegliere fra JSON e YAML; la scelta e' **JSON Lines** per due ragioni che
   vengono entrambe dal volume che il file vede a `-vvv` (3,5 MB, ~12.000 record per un solo job):
   *streaming* — ogni record raggiunge il writer bufferizzato quando accade, quindi niente si
   accumula in memoria e il log di un processo morto a meta' resta leggibile, cosa che un array
   JSON o un documento YAML, che vanno chiusi in fondo, non garantiscono — e *indipendenza della
   riga*, che tiene funzionanti `grep` e `jq`.
2. **stderr non porta piu' il `target`** (il percorso del modulo). Era il token piu' lungo della
   riga ed e' quasi mai cio' che si cerca leggendo una corsa dal vivo. Non e' perso: e' un campo
   di ogni record del file, dove si puo' filtrare.
3. **`.log.csv` non compare piu' nella cartella corrente.** Non era una svista di L4 ma il suo
   fallback: `LogHandle::close` ripiegava sulla cartella data a `init` quando la destinazione non
   era stata fissata, cosa che accade a ogni corsa che fallisce *prima* della risoluzione della
   configurazione. Riprodotto con `freeports -i /nonexistent/x.pdf -f NOPE -o ./outdir`, che
   lasciava in cwd un `.log.csv` di 80 byte (sola intestazione). Il fallback e' stato tolto:
   nessuna destinazione, nessun file (`CsvLogLayer::discard`). Le righe in sospeso non sono
   davvero perse — sono eventi che hanno gia' raggiunto stderr e il log su file.
4. **Quattro colori nel percorso degli span**, non uno: nomi in ciano, valori fra parentesi in
   magenta chiaro, `/` e `[`/`]` in grigio scuro, campi dell'evento in dim. Tinte diverse e non
   sfumature della stessa, perche' la richiesta e' distinguere segmenti e parametri *a colpo
   d'occhio*.
5. **L'errore serializzato nel log su file**: `LogRecord` e `build_record` sono ora condivisi fra
   `JsonLogLayer` e `YamlLogLayer`, quindi il `.jsonl` porta lo stesso `ErrorRecord`
   (`debug`/`display`/catena di `source()`) che L3 aveva introdotto per il solo YAML. Lo sweep dei
   53 siti fatto in L3 lo popola gia'; l'unico sito rimasto scoperto, `python::utils::pdf_extract::
   value_error`, e' stato sdoppiato (`value_error` per i veri `std::error::Error`,
   `value_error_msg` per l'unico chiamante che passa una `String`), cosi' che anche i suoi 9 siti
   serializzino l'errore.

**Ridondanza da decidere con l'utente.** `.freeports.log.yaml` (L3) e' ora un *sottoinsieme* di
`freeports.log.jsonl`: a `-vvv` il secondo contiene gia' tutti i record `warn`/`error` con
l'errore serializzato, piu' tutto il resto. Il primo resta perche' e' piccolo e leggibile da un
umano, ma la sovrapposizione va detta, non nascosta.

**Nota, non cambiata.** stderr emette le sequenze ANSI anche quando non e' un terminale (comportamento
di L4, non introdotto qui): con quattro colori invece di uno, un `2>file` ne raccoglie
sensibilmente di piu'. Un `std::io::IsTerminal` su stderr lo risolverebbe, ma cambia cio' che
l'utente vede quando redirige, quindi non e' stato fatto di iniziativa.

---

## 4. Fase P — parallelizzazione (richiesta 1)

### P0. Prima di parallelizzare: misurare

Nessuna delle scelte qui sotto va fatta a intuito. Il primo passo produce un profilo su almeno
tre report reali di taglia diversa (per esempio MEDIOLANUM-ES24.B: 29 pagine; UBS-EN23: 222;
AMUNDI-EN24: 1.824), che risponda a tre domande:

1. quanto pesa `input::document::load_document_pages` (PyMuPDF, **sotto GIL**) sul totale;
2. quanto pesa la classificazione rispetto agli step;
3. quanto pesano i tre segmenti l'uno rispetto all'altro, e quanto un singolo pipe.

Senza questi numeri, P1..P4 sono ipotesi. Con questi numeri, meta' delle ipotesi si cancella.

### Il vincolo che decide tutto: il GIL

Il crate tocca Python in due punti (`PLAN.md` storico §3), ed entrambi sono rilevanti qui:

- **caricamento del PDF** — `load_document_pages` cicla su **tutte** le pagine dentro un solo
  `Python::attach`, chiamando `load_page`/`get_text("dict")`. PyMuPDF non rilascia il GIL: su un
  documento da 1.824 pagine questo e' un blocco seriale che **nessun thread puo' accelerare**;
- **pipe definiti dall'autore** (formati `unstructured`) — ogni chiamata riprende il GIL, quindi
  N thread su pipe Python si serializzano fra loro.

Conseguenze dirette:

- i **thread** pagano solo dove il lavoro e' Rust puro: `pdf_extract`/`text_filter`/`deserialize`
  nativi, tabularizer, matching, regex `onig`, cast. Per i formati `structured` e
  `semistructured` — la maggioranza — e' la parte grossa;
- per i formati `unstructured` la parallelizzazione a thread **non dara' guadagno**, e va rilevata
  e degradata a sequenziale invece di pagare l'overhead per niente;
- l'unico modo per parallelizzare davvero *anche* la parte Python e' il **multi-processo**, che ha
  senso solo al livello piu' grosso (P1), dove i job sono gia' indipendenti e non c'e' nulla da
  ricomporre in memoria.

### P1. Livello job / documento — **processi**, il guadagno maggiore

In modalita' batch, `cli::batch::load_jobs` produce N `PartialConfig` indipendenti e
`cli::run::execute` li esegue in un `for` sequenziale. Ogni job carica il proprio PDF, esegue il
proprio algoritmo, scrive i propri CSV: **nessuna memoria condivisa**.

Proposta: eseguire i job in **processi figli** (`std::process::Command` sul proprio eseguibile con
la config del job, oppure `fork` non e' portabile — meglio il primo), con un pool di dimensione
`jobs`. E' l'unico livello che scavalca il GIL, e in batch e' anche quello con il rapporto
guadagno/rischio migliore.

Da risolvere: raccolta dei log dei figli (ogni figlio scrive il proprio `.log.csv`? si uniscono a
fine corsa? — l'unione va fatta **ordinata**, vedi L1), propagazione del codice d'uscita, e cosa
succede se due job scrivono nella stessa cartella di output. **Q-P1**.

Alternativa piu' semplice, da valutare in P0: thread anche qui, accettando che i tratti Python si
serializzino. Piu' semplice da implementare e da diagnosticare, guadagno parziale.

### P2. Livello pagina — **thread (rayon)**, il guadagno strutturale

Due punti, entrambi in `core::algorithm`:

- **`classify_pages`** (righe ~202-230): cicla su tutte le pagine del documento applicando le
  pipeline di classificazione. E' il punto che la richiesta cita per primo ("le deve parsare tutte
  quindi e' lungo") ed e' esatto: mediana 288 pagine, punta 1.824. I contributi vanno raccolti
  **in ordine di pagina** perche' il finalizer riceve un `Vec<Option<PageClass>>` posizionale:
  quindi `par_iter().map(...).collect()`, mai `for_each` + push.
- **il ciclo sulle pagine di uno step** (righe ~269-305): le pagine dello stesso step sono
  indipendenti per costruzione — un test esistente lo garantisce esplicitamente
  (`pages_of_the_same_step_do_not_see_each_others_results`). E' la parallelizzazione piu' naturale
  di tutto il motore.

Precondizioni gia' soddisfatte (verificate, non assunte): i tre trait dei pipe sono `Send + Sync`
per scelta esplicita di M5, `Page` e' `Send + Sync`, `Algorithm` e i bundle sono dietro `Arc`.

Attenzione a tre cose:
1. **`Page::raw` e' un `Py<PyAny>`**: il suo `Drop` richiede il GIL. Va verificato che la
   distruzione di pagine su thread rayon non prenda il GIL a raffica (eventualmente rilasciando i
   `raw` in un punto solo).
2. **Ordine dei risultati**: `produced_in_this_step` e `per_page` devono risultare identici al
   caso sequenziale. E' il vincolo di determinismo di §6.
3. **Span**: ogni closure deve riagganciare lo span del chiamante (`let span =
   tracing::Span::current(); ... span.in_scope(|| ...)`), altrimenti la colonna `Activity` si
   svuota proprio dove serve.

### P3. Livello page class / pipeline dentro uno step

La richiesta chiede di parallelizzare "per le diverse page_class se sono nello stesso step" e "per
pipeline" dentro un bundle. Tecnicamente e' possibile (stessi trait `Send + Sync`), ma:

- le page class dentro uno step sono tipicamente **1-3**, e le pagine sono centinaia: e' P2 che
  satura i core, non P3;
- annidare rayon dentro rayon non e' un errore (il pool e' work-stealing e gestisce il nesting),
  ma rende il profilo illeggibile e il determinismo piu' delicato.

Raccomandazione: **implementarlo, ma con default disattivato** (`pipelines = 1`), utile per il
caso patologico "un documento con pochissime pagine e molte pipeline pesanti". Attivabile da
configurazione.

### P4. Livello pipe dentro un segmento — la risposta e' "quasi mai"

L'utente lo dice esplicitamente ("non so bene... se ci fosse un modo semplice di aiutare il
compilatore"). Risposta netta, perche' e' la parte dove l'intuizione inganna:

- **il compilatore Rust non parallelizza da solo**. Non esiste un attributo che renda parallelo un
  ciclo; `rustc` auto-vettorizza (SIMD) solo cicli numerici semplici, e qui il lavoro e' stringhe,
  regex Oniguruma e `HashMap` — niente da vettorizzare. Non c'e' un "modo semplice di dirlo al
  compilatore": o si usano i thread, o e' sequenziale;
- **i pipe di un segmento sono pochi** (tipicamente 1-5) e ciascuno costa poco: il costo di
  distribuzione di rayon (decine di microsecondi) e' dello stesso ordine del lavoro;
- l'**unica** eccezione con numeri veri e' `DeserializeSegment::apply`, che cicla
  **pipe x blocchi** e i blocchi di una pagina densa sono centinaia o migliaia. Li' un
  `par_iter` sui *blocchi* (non sui pipe) con una **soglia** (sotto N blocchi resta sequenziale)
  puo' pagare — ma solo se P0 lo mostra.

Quindi: P4 = una sola parallelizzazione, condizionata a soglia, sui blocchi di `deserialize`, e
solo se misurata. Tutto il resto resta sequenziale per scelta, non per dimenticanza.

### P5. Configurazione

`n_workers` esiste gia' (config, `--workers/-j`, `FREEPORTS_N_WORKERS`) e non e' usato: diventa il
default globale. Sopra ci va una sezione dedicata, con override per livello:

```yaml
parallelism:
  jobs: auto        # P1 — processi in batch.     auto = min(n_cpu, n_job)
  pages: auto       # P2 — thread rayon.          auto = n_cpu
  pipelines: 1      # P3 — disattivato di default
  deserialize_blocks_threshold: 0   # P4 — 0 = disattivato
```

Regole: `auto` risolve a runtime; `1` ovunque deve produrre **esattamente** il comportamento
sequenziale di oggi (ed e' il modo di verificare il determinismo, §6); `--workers/-j N` senza
altro imposta `pages = N` e lascia il resto al default. Le variabili d'ambiente seguono lo schema
gia' esistente in `cli::config_locations::env`.

---

## 5. Fase D — documentazione (richiesta 3)

### D1. La strategia (da decidere per prima)

**Cosa c'e' oggi**: `docs/` con Sphinx (`sphinx_rtd_theme`, `autodoc`, `autosummary`,
`napoleon`, `intersphinx`, `coverage`), ~9.000 parole di prosa vera in `usage/`, `dev/`,
`validation/`, **quattro locali gettext** (`en`, `fr`, `it`, `pt`) e un `.readthedocs.yaml`.
E `docs/source/_generated/*.rst` che documenta via autodoc un pacchetto — `freeports_analysis` —
che **non esiste piu' da due riscritture**; `conf.py` fa `from freeports_analysis import *`, quindi
oggi la build o e' rotta o e' vuota.

**Cosa serve domani**, tre pubblici distinti:

| Pubblico | Contenuto | Strumento naturale |
|---|---|---|
| Chi sviluppa il crate | API Rust, modulo per modulo | **rustdoc** (`cargo doc`), generato dai doc-comment di D2 |
| Chi scrive un formato | API Python esposta da PyO3, guide "come si fa un formato / un repo formati / un input-db" | **Sphinx** (autodoc funziona sul modulo compilato, e' importabile) |
| Chi valuta il progetto | Whitepaper: installazione, uso, scelte di design e perche', metodologia di validazione | **prosa**, in uno dei due |

**Raccomandazione: un sito Sphinx solo, piu' rustdoc pubblicato accanto**, non mdbook. Motivi:

1. le **traduzioni gia' esistono** in quattro lingue con il flusso gettext: mdbook le rifarebbe da
   zero con un secondo meccanismo (`mdbook-i18n-helpers`);
2. `validation/` (metodologie, asserzioni, grant) e' gia' li' ed e' contenuto vivo, non
   riscrivibile a costo zero;
3. `.readthedocs.yaml` e la pubblicazione esistono gia';
4. abilitando **MyST** la prosa nuova si scrive in Markdown dentro Sphinx: si ottiene la comodita'
   di scrittura di mdbook senza un secondo toolchain;
5. rustdoc non va integrato ne' duplicato: si genera con `cargo doc` e si pubblica come
   sotto-percorso, linkato dall'indice. Duplicare l'API Rust in `.rst` a mano invecchierebbe in
   una settimana.

Da fare comunque, indipendentemente dalla scelta: **cancellare `docs/source/_generated/`** e
riparare `conf.py` (che oggi importa un pacchetto morto). **Q-D2** per la conferma della
strategia e per la sorte delle traduzioni.

### D2. Doc-comment del sorgente

I doc-comment attuali sono, per ammissione della richiesta, "traccie operative" scritte durante il
porting: contengono contratti per l'implementatore, riferimenti a `M9-implementation-plan.md §0
Q5`, tabelle di test, note su cosa era rimandato a quale milestone. Servivano ad allora; oggi sono
rumore per chi legge il codice.

Regola di riscrittura, modulo per modulo:

- **resta**: cosa fa il modulo, quali invarianti garantisce, come si usa, perche' e' fatto cosi'
  dove la scelta non e' ovvia, i limiti noti;
- **sparisce**: riferimenti a milestone e a piani, contratti "l'implementazione deve...", blocchi
  di codice che descrivono firme gia' presenti sotto, cronologia delle decisioni (che vive in
  `agent-memory/` e in questo file);
- **si aggiunge**: esempi eseguibili (`/// ```` `) dove il tipo e' non banale — diventano doc-test,
  quindi documentazione che non puo' invecchiare in silenzio;
- **lingua: inglese**, coerentemente con F1 (**Q-F1**).

Nota di processo: nella convenzione di questo workspace i **commenti nel codice sono compito di
`implementer`, non di `docs-writer`** (`CLAUDE.md`: docs-writer "Not for writing in-code comments").
D2 va quindi eseguita come lavoro di implementazione, area per area, non come lavoro di
documentazione.

### D3. Whitepaper

Documento in prosa, per umani, che spieghi didatticamente: cos'e' il problema (estrarre dati
strutturati da report finanziari PDF eterogenei), installazione, uso da CLI e da Python, il
modello di esecuzione (documento -> pagine -> classificazione -> schedule -> page class ->
pipeline -> tre segmenti -> output, e da dove viene questa forma), **come si scrive un formato**,
**come si crea e si mantiene un repo dei formati**, **come si fa un input-db**, il sistema dei
grant/validazione, e le scelte di design con le alternative scartate (perche' Rust, perche'
Oniguruma e non `regex`, perche' PyMuPDF resta l'unico ponte Python, perche' le promesse, perche'
tre segmenti e non quattro).

Materiale gia' esistente da cui attingere, non da riscrivere da zero: `docs/source/dev/code.rst`
(1.768 parole), `docs/source/usage/command.rst` (1.304), `docs/source/validation/**` (~2.500), e
il `PLAN.md` storico §2/§12/§13, che contiene gia' le motivazioni delle scelte in forma
discorsiva.

### D4. Riporto e riconciliazione dei contenuti Sphinx esistenti

Passata di verifica su ogni `.rst` non generato: ancora vero? riferito a nomi vivi? Le parti su
`freeports_analysis`/`freeports_core` vanno riscritte sui nomi attuali, quelle su comandi e
config vanno riallineate a `cli::config_locations` (che nel frattempo ha cambiato semantica su
`-v`/`-q`), quelle di validazione controllate contro `freeports_validate`.

---

## 6. Invarianti che valgono per tutte e quattro le fasi

1. **`tests/formats/*/out/**` non si tocca.** Sono la specifica eseguibile. Se l'output diverge, ha
   sbagliato il motore. L'unica eccezione possibile e' `out/.log.csv` in fase L, e **solo** con
   autorizzazione esplicita (Q-L1).
2. **Determinismo**: con `parallelism` a 1 ovunque, l'output deve essere **identico byte per byte**
   a quello di oggi; con N > 1, identico a quello con 1. Da rendere un test, non una speranza:
   stessa corsa a 1 e a N worker, confronto dei file prodotti.
3. **Nessuna regressione dei test**: 2.474 unitari + 63 d'integrazione + 259 del repo formati.
4. **Stile dei test**: sottomoduli per argomento dentro `mod tests`, mai una lista piatta.
5. **Cambiamenti al codice del repo formati**: si propongono, non si applicano.
6. **Bug ereditati**: si correggono alla radice, ma sempre chiedendo prima, offrendo l'opzione
   "parametro opt-in con il vecchio comportamento come default".

---

## 7. Domande aperte — bloccanti

| # | Fase | Domanda |
|---|---|---|
| **Q-F1** | F/D | "Tutto in inglese" comprende anche i ~2.961 righe di doc-comment italiani? Raccomandazione: si', ma tradotti dentro D2 (che li riscrive comunque), non in F1 — cosi' non si toccano due volte. |
| **Q-F2** | F | ~~Quale forma per la correzione dell'ambiguita' `BlockValue::List` nelle promesse: (a) contributi separati dal valore, (b) variante `Multi`, (c) parametro opt-in?~~ **Risposta 2026-08-29: (a).** |
| **Q-F3** | F | ~~Confermi che la rigenerazione riguarda i 228 JSON a pagina singola di tutti i 26 formati, e che `out/**` resta intatto?~~ **Risposta 2026-08-29: si'.** |
| **Q-L1** | L | ~~Il nuovo schema di `.log.csv` **invalida i 31 `tests/formats/*/out/.log.csv`**, che sono file "che non si toccano". Autorizzi la loro rigenerazione una tantum come parte di L1 (e con quale verifica), oppure il motore deve poter scrivere anche il formato vecchio (flag di compatibilita')?~~ **Risposta 2026-08-29: rigenerazione una tantum**, verifica sul modello di F3 (checksum sul contenuto equivalente, revisione del delta). |
| **Q-L2** | L | ~~Nome e posizione della colonna dello span (proposto: `Activity`, seconda); vocabolario e separatore degli span (proposto: `/`); e soprattutto: la presenza di `Activity` da sola basta a generare una riga in `.log.csv`?~~ **Risposta 2026-08-29: no**, come raccomandato — `Activity` arricchisce la riga, non la giustifica da sola. |
| **Q-L3** | L | `.freeports.log.yaml`: solo `-vvv` o flag dedicato? solo errori/warning o tutti gli eventi? record strutturale (raccomandato) o `Serialize` derivato su ~25 enum d'errore? |
| **Q-P1** | P | Il livello job puo' usare **processi figli** (unico modo di scavalcare il GIL) con la complessita' che comporta — unione ordinata dei log, codici d'uscita, cartelle di output condivise — o si resta ai soli thread accettando il guadagno parziale? |
| **Q-P2** | P | Confermi il vincolo di determinismo byte-per-byte (§6.2)? E' cio' che esclude le soluzioni piu' rapide (raccolta non ordinata) e va deciso prima di scrivere il codice. |
| **Q-D1** | D | Il whitepaper e' rivolto anche a un pubblico non tecnico (finanziario/istituzionale) o solo a sviluppatori? Cambia registro e struttura. |
| **Q-D2** | D | Confermi "un solo sito Sphinx + MyST + rustdoc accanto", scartando mdbook? E le quattro traduzioni (`en`/`fr`/`it`/`pt`) si mantengono, si congelano, o si riducono? |

---

## 8. Agenti e modelli consigliati

Legenda: **O** = Opus (ragionamento architetturale, rischio alto, ambiguita' reale) · **S** =
Sonnet (lavoro definito, ampio, meccanico o semi-meccanico) · **H** = Haiku (inventari, grep,
conteggi).

| Fase | Passo | Agente | Modello | Perche' |
|---|---|---|---|---|
| F1 | Inventario italiano->inglese | `Explore` | H | e' solo ricerca; il risultato e' una lista |
| F1 | Rinomine | `refactorer` | S | comportamento invariato per definizione, e' riorganizzazione |
| F2 | Analisi dell'ambiguita' promesse | `requirements-analyst` | **O** | e' una scelta di modello dati con effetti su serializzazione, API e fixture |
| F2 | Contro-analisi | `critic` | **O** | prima di cambiare un tipo che attraversa tutto il motore |
| F2 | Test poi implementazione | `test-writer` -> `implementer` | O -> S | i test sono la parte difficile; l'implementazione segue |
| F3 | Script di rigenerazione + rimozione legacy | `implementer` | S | meccanico, ma con la precondizione "integrazione verde" da rispettare |
| L1 | Schema `.log.csv` + span + determinismo | `implementation-planner` | **O** | qui si decide una struttura che P eredita; sbagliarla costa due volte |
| L1 | Test poi implementazione del layer | `test-writer` -> `implementer` | **O** -> **O** | layer `tracing` multi-thread con ordinamento: e' codice sottile |
| L2 | Sweep di strumentazione, area per area | `implementer` | S | 116 file, convenzione gia' scritta: volume, non difficolta'. Un'area per sessione |
| L2 | Verifica del rumore prodotto | `critic` | S | leggere un `.log.csv` reale e dire cosa e' inutile |
| L3 | `.freeports.log.yaml` | `implementation-planner` -> `test-writer` -> `implementer` | O -> O -> S | la forma del record e' progetto; il resto e' esecuzione |
| P0 | Profilo su 3 report reali | `implementer` (o sessione diretta) | S | e' misura: `cargo build --release`, `perf`/timing, tabella |
| P1-P4 | Requisiti e strategia | `requirements-analyst` | **O** | GIL, processi vs thread, determinismo: e' la decisione piu' rischiosa del piano |
| P1-P4 | Piano | `implementation-planner` | **O** | |
| P1-P4 | Revisione avversariale | `critic` | **O** | data race e non-determinismo non li trova un test scritto distrattamente |
| P1-P4 | Test poi implementazione | `test-writer` -> `implementer` | **O** -> **O** | |
| P5 | Configurazione | `implementer` | S | segue schemi gia' esistenti in `cli::config_locations` |
| D1 | Strategia documentale | `requirements-analyst` + `docs-writer` | **O** | decisione strutturale, e riguarda anche le traduzioni |
| D2 | Doc-comment, area per area | `implementer` | S | per convenzione del workspace i commenti sono di `implementer`, non di `docs-writer` |
| D3 | Whitepaper | `docs-writer` | **O** | prosa didattica lunga con motivazioni: e' il lavoro dove il modello si sente |
| D4 | Riporto Sphinx | `docs-writer` | S | riconciliazione di testo esistente |
| — | Skill/permessi nuovi (es. rigenerazione fixture) | `tool-smith` | S | |

**Note d'uso.**

- Le fasi **F1**, **L2** e **D2** sono *sweep*: molto volume, poca ambiguita'. Vanno spezzate per
  area (`cli`, `input`, `formats_repo`, `core`, `formats_utils`, `output`, `commons`) e affidate
  una per volta, non tutte insieme.
- Le fasi **F2**, **L1**, **L3** e tutta **P** hanno ambiguita' vera: li' l'ordine
  `requirements-analyst -> (domande all'utente) -> implementation-planner -> critic -> test-writer
  -> implementer` va rispettato per intero, e conviene Opus fino a `test-writer` compreso.
- Ogni agente di questo workspace ha istruzione di **chiedere all'utente** quando incontra un
  giudizio non suo: le domande di §7 vanno risposte prima, non aggirate.

---

## 9. Rischi

1. **Q-L1 e' un vero conflitto di regole**, non un dettaglio: il nuovo schema `.log.csv` e la
   regola "gli output di riferimento non si toccano" non possono essere veri insieme. Va risolto
   dall'utente prima di scrivere una riga di L1.
2. **La parallelizzazione puo' rendere i test instabili** (flaky) invece che falsi: un test che
   passa 9 volte su 10 e' peggio di uno rosso. Il vincolo di determinismo (§6.2) e il test
   "1 worker vs N worker" servono esattamente a questo.
3. **Il GIL puo' azzerare il guadagno atteso** sui formati `unstructured`. Se P0 mostra che il
   caricamento PyMuPDF domina, P2 dara' molto meno del previsto e il grosso del lavoro dovra'
   spostarsi su P1 (processi).
4. **Lo sweep di logging puo' produrre rumore** invece di informazione: 116 file strumentati senza
   convenzione diventano un `.log.csv` illeggibile. La convenzione di L2 va fissata e verificata
   su un'area sola prima di applicarla alle altre sei.
5. **La documentazione invecchia**: il whitepaper scritto prima che P sia chiusa descrivera' un
   motore che non esiste. E' il motivo per cui D e' ultima.
