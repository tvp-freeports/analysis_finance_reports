# `freeports` — stato della riscrittura

File di continuità fra sessioni. **Va aggiornato alla chiusura di ogni milestone**, prima di
considerare il lavoro finito. Il piano è in `PLAN.md`; qui c'è solo *dove siamo*.

## Stato per milestone

| # | Milestone | Stato | Note |
|---|---|---|---|
| M0 | Scaffolding | 🟡 parziale | albero moduli e `Cargo.toml` fatti, `cargo check` passa. Mancano: `tracing_setup`, tipi d'errore base, layer `.log.csv` |
| M1 | `commons` | ⬜ da fare | |
| M2 | `core` dati | ⬜ da fare | |
| M3 | `pdf_extract` | ⬜ da fare | porting verbatim, vedi `PLAN.md` §0 |
| M4 | `text_filter` + `deserialize` | ⬜ da fare | |
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

## Domande aperte

Vedi `PLAN.md` §13. In sintesi: semantica di `FilterData` (blocca M5), rigenerazione dei fixture
`freeports-dev` (M8 o M10), colonne di `.log.csv`.
