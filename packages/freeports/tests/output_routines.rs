//! Test d'integrazione di `output::routines` (M8, passo 16 di
//! `agent-memory/M8-implementation-plan.md` §3).
//!
//! Repo-formati-free: costruisce `DocumentOutcome`/`PageOutcome` a mano (senza passare da
//! `Algorithm`/`formats_repo`), incluse promesse pendenti da risolvere attraverso più pagine e più
//! documenti, poi verifica che i CSV scritti in un `tempfile::TempDir` siano corretti byte per
//! byte — lo stesso focus di test dichiarato da `PLAN.md` §11 per M8, qui end-to-end
//! (`accumulate` + `write_files` insieme) invece che modulo per modulo.
//!
//! Percorsi interni (`freeports::core::...`, `freeports::output::...`), non `freeports::api`:
//! `api::output`/`api::core` non riesportano ancora (per questa milestone) i tipi di
//! `output::routines`, che non fanno parte di `PLAN.md` §9 — stesso trattamento già riservato a
//! `api::input`/`api::formats_repo` (gap fra §9 e lo scope necessario, documentato non silenzioso,
//! vedi `STATUS.md`).

use freeports::commons::consts::Currency;
use freeports::core::algorithm::{DocumentOutcome, PageOutcome};
use freeports::core::classes::value::BlockValue;
use freeports::core::page::{DocumentId, FormatName};
use freeports::core::pipeline::{Extracted, PromiseEntries};
use freeports::core::promise::Promise;
use freeports::core::schedule::PageClass;
use freeports::output::classes::fund::Fund;
use freeports::output::classes::investment::{Equity, InvestmentFields};
use freeports::output::routines::accumulate::accumulate;
use freeports::output::routines::write::{OutFlags, OutStructureMode, write_files};

fn page(number: u32, class: &str, results: Vec<Extracted>) -> PageOutcome {
    PageOutcome { page: number, class: PageClass::new(class), results }
}

fn equity_with_promised_market_value(fund: &str, promise_id: &str) -> Extracted {
    Extracted::Equity(
        Equity::build(InvestmentFields::new(
            "Acme Corp",
            "Acme",
            BlockValue::from(fund),
            BlockValue::from(Promise::new(promise_id)),
            BlockValue::from(Currency::EUR),
        ))
        .expect("fixed, valid fixture"),
    )
}

fn promises(entries: Vec<(&str, BlockValue)>) -> Extracted {
    Extracted::Promises(entries.into_iter().collect::<PromiseEntries>())
}

#[test]
fn a_two_document_run_with_a_cross_page_promise_writes_the_expected_csvs() {
    // Documento 1: pagina 1 deposita un `Equity` con `market_value` pendente; pagina 2 (una
    // pagina diversa dello *stesso* documento) deposita la promessa che lo risolve, più un
    // `Fund` autonomo — il fondo compare quindi sia direttamente (pagina 2) sia indirettamente
    // (via `Equity.fund`, pagina 1): deve finire in una sola riga di `funds.csv`, con il
    // `Report page` della sua comparsa diretta.
    let doc_one = DocumentOutcome {
        id: DocumentId::new("Report One"),
        format: FormatName::new("STD"),
        pages: vec![
            page(1, "investments", vec![equity_with_promised_market_value("Alpha Fund", "mv-id")]),
            page(
                2,
                "funds",
                vec![Extracted::Fund(Fund::new("Alpha Fund")), promises(vec![("mv-id", BlockValue::from(1500.0))])],
            ),
        ],
    };

    // Documento 2: un secondo report, indipendente, con un solo investimento su un fondo diverso.
    let doc_two = DocumentOutcome {
        id: DocumentId::new("Report Two"),
        format: FormatName::new("STD"),
        pages: vec![page(
            1,
            "investments",
            vec![Extracted::Equity(
                Equity::build(InvestmentFields::new(
                    "Other Corp",
                    "Other",
                    BlockValue::from("Beta Fund"),
                    BlockValue::from(250.0),
                    BlockValue::from(Currency::USD),
                ))
                .unwrap(),
            )],
        )],
    };

    let tables = accumulate(&[doc_one, doc_two]).expect("all promises resolve, no duplicates");

    assert_eq!(tables.investments.len(), 2);
    assert_eq!(tables.funds.len(), 2);

    let alpha = tables.funds.iter().find(|f| f.name == "ALPHA FUND").expect("Alpha Fund must be present");
    assert_eq!(alpha.report_page, Some(2), "Alpha Fund's debug info comes from its direct sighting");
    let alpha_investment = tables.investments.iter().find(|r| r.fund_id == alpha.id).unwrap();
    assert_eq!(alpha_investment.market_value, 1500.0, "the promised market value must be resolved before writing");

    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

    let investments_csv = std::fs::read_to_string(out.join("investments.csv")).unwrap();
    let mut lines: Vec<&str> = investments_csv.lines().collect();
    assert_eq!(
        lines.remove(0),
        "ID,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency"
    );
    assert_eq!(lines.len(), 2, "both investments must be written, promise already resolved");
    assert!(
        lines.iter().any(|l| l.contains("Acme Corp") && l.contains("1500.0") && l.contains("Report One")),
        "the resolved Alpha Fund investment must be present: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("Other Corp") && l.contains("250.0") && l.contains("Report Two")),
        "the Beta Fund investment from the second document must be present: {lines:?}"
    );

    let funds_csv = std::fs::read_to_string(out.join("funds.csv")).unwrap();
    assert!(funds_csv.contains("ALPHA FUND"));
    assert!(funds_csv.contains("BETA FUND"));

    // Ogni altro file previsto dal profilo `Regular` esiste comunque, anche se vuoto.
    for name in [
        "funds_assets.csv",
        "funds_sfdr_classification.csv",
        "funds_esg_indicators.csv",
        "assets_managers.csv",
        "investments_managers_to_funds.csv",
        "funds_change_name.csv",
        "investments_add_infos.yaml",
    ] {
        assert!(out.join(name).is_file(), "missing {name}");
    }
}

#[test]
fn a_non_strict_unresolvable_promise_drops_only_the_entity_that_depends_on_it() {
    let doc = DocumentOutcome {
        id: DocumentId::new("Report"),
        format: FormatName::new("STD"),
        pages: vec![
            page(1, "investments", vec![equity_with_promised_market_value("Alpha Fund", "mv-id")]),
            page(
                2,
                "investments",
                vec![Extracted::Equity(
                    Equity::build(InvestmentFields::new(
                        "Other Corp",
                        "Other",
                        BlockValue::from("Beta Fund"),
                        BlockValue::from(1.0),
                        BlockValue::from(Currency::EUR),
                    ))
                    .unwrap(),
                )],
            ),
            // Nessuna promessa depositata per "mv-id": l'Equity di pagina 1 sparisce, quello di
            // pagina 2 no.
        ],
    };

    let tables = accumulate(&[doc]).unwrap();
    assert_eq!(tables.investments.len(), 1);
    assert_eq!(tables.investments[0].investee, "Other Corp");
}
