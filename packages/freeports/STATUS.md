# `freeports` — stato: parallelizzazione, logging, documentazione, correzioni

File di continuita' fra sessioni. **Va aggiornato alla chiusura di ogni passo**, prima di
considerare il lavoro finito. Il piano e' in `PLAN.md`; qui c'e' solo *dove siamo*.

Lo stato della riscrittura precedente (M0..M10, tutte chiuse) e' recuperabile da git:
`git show 13284baa:packages/freeports/STATUS.md`.

## Stato per passo

| # | Passo | Stato | Note |
|---|---|---|---|
| **F1** | Tutto in inglese (identificatori e test) | 🟡 in corso | I 6 file concentrati (~187 nomi di funzione/test) sono fatti: `core/{promise,promisable,promise_resolution,classes,classes/value,match_fund,normalization}.rs` (identificatori, nomi di test/sottomodulo, campi privati; doc-comment e commenti `//` lasciati in italiano, per D2). `cargo test --lib core::` -> 447 passati, 0 falliti. Restano gli ~30 file con variabili locali italiane sparse nel resto del crate (`appiattiti`, `contributi`, `candidati`, `unico`, `in_corso`, `valori`, `attesa`, `riga`, ...) — inventario ancora da fare. Doc-comment esclusi (vanno in D2, vedi Q-F1) |
| **F2** | Ambiguita' `BlockValue::List` nella multimap delle promesse | ⬜ da fare | Bug confermato leggendo `core/promise_resolution.rs` (`flatten` ~145-152, `fulfill` ~193-205): un contributo che *e'* una lista e' indistinguibile da N contributi scalari. Bloccato su **Q-F2** |
| **F3** | Fixture a pagina singola rigenerate, `_LEGACY_MODULES` rimosso | ⬜ da fare | 76 pagine x 3 file = 228 JSON in 26 formati, 175 con tag legacy. `out/**` non si tocca. Bloccato su **Q-F3** |
| **L1** | Nuovo schema `.log.csv` (colonna `Activity`, coordinate generalizzate, righe ordinate) | ⬜ da fare | **Bloccato su Q-L1**: il nuovo schema invalida i 31 `tests/formats/*/out/.log.csv`, che per regola non si toccano |
| **L2** | Strumentazione capillare, 7 aree | ⬜ da fare | Oggi 19 log e 3 span in tutto il crate. Convenzione per livello in `PLAN.md` §3 L2 |
| **L3** | `.freeports.log.yaml` a verbosita' massima | ⬜ da fare | Quarto layer accanto ai tre esistenti. Bloccato su **Q-L3** |
| **P0** | Profilo su 3 report reali | ⬜ da fare | Nessun passo P parte prima di questo |
| **P1** | Job/documento — processi | ⬜ da fare | Unico livello che scavalca il GIL. Bloccato su **Q-P1** |
| **P2** | Pagina — thread rayon (classificazione + step) | ⬜ da fare | Il guadagno strutturale: report con mediana 288 pagine, punta 1.824 |
| **P3** | Page class / pipeline dentro uno step | ⬜ da fare | Da implementare ma **disattivato di default** |
| **P4** | Blocchi di `deserialize` sopra soglia | ⬜ da fare | Solo se P0 lo giustifica. Sui *pipe* non si parallelizza: vedi `PLAN.md` §4 P4 |
| **P5** | Configurazione `parallelism` | ⬜ da fare | `n_workers` esiste gia' in config e in `--workers/-j` ed e' **inutilizzato**: diventa il default globale |
| **D1** | Strategia documentale (Sphinx+MyST+rustdoc vs mdbook) | ⬜ da fare | Bloccato su **Q-D2**. `docs/source/_generated/` documenta un pacchetto morto (`freeports_analysis`) e va cancellato in ogni caso |
| **D2** | Doc-comment riscritti, area per area | ⬜ da fare | 6.103 righe di doc-comment, ~2.961 in italiano. Per convenzione del workspace e' lavoro di `implementer`, non di `docs-writer` |
| **D3** | Whitepaper | ⬜ da fare | Materiale di partenza: `docs/source/{dev/code,usage/command,validation/**}.rst` + `PLAN.md` storico §2/§12/§13 |
| **D4** | Riporto e riconciliazione dei contenuti Sphinx | ⬜ da fare | ~9.000 parole di prosa esistente, 4 locali gettext |

Legenda: ⬜ da fare · 🟡 in corso · ✅ chiusa (test verdi, `STATUS.md` aggiornato)

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
| Q-F3 | F3 | Rigenerazione dei 228 JSON confermata, `out/**` intatto? |
| Q-L1 | **L1, e a cascata L2/L3** | I 31 `out/.log.csv` si possono rigenerare, o serve una modalita' di compatibilita'? |
| Q-L2 | L1 | Nome/posizione della colonna span; `Activity` da sola basta a generare una riga? (racc.: no) |
| Q-L3 | L3 | YAML: quando si genera, cosa contiene, record strutturale o `Serialize` sugli enum? |
| Q-P1 | P1 | Processi figli ammessi, o solo thread? |
| Q-P2 | P2, P3, P4 | Determinismo byte-per-byte confermato come vincolo? |
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

## Invarianti (non negoziabili senza l'utente)

1. `tests/formats/*/out/**` non si tocca — unica eccezione possibile `out/.log.csv`, solo con
   autorizzazione esplicita (Q-L1).
2. Con `parallelism` a 1 l'output e' identico byte per byte a quello di oggi; con N e' identico a
   quello con 1.
3. Nessuna regressione: 2.474 unitari + 63 d'integrazione + 259 del repo formati.
4. Test Rust raggruppati per argomento in sottomoduli, mai lista piatta.
5. Modifiche al codice del repo formati: si propongono, non si applicano.
6. Bug ereditati: correzione alla radice, ma chiedendo prima, offrendo l'opzione "parametro opt-in
   con il vecchio comportamento come default".
