//! `FundSfdrClassification`: la classificazione SFDR (art. 6/8/9) dichiarata di un fondo.
//!
//! M8, passo 3 (`agent-memory/M8-implementation-plan.md` §3). Il pipe più semplice degli otto
//! deferiti (`DeserializeSfdrArticleStandard`, passo 8) costruisce esattamente questa entità da un
//! blocco `SFDR_ARTICLE`. Vedi
//! `packages/freeports_core/src/output/classes/fund_sfdr_classification.rs` per il riferimento:
//! `fund` non è mai promettibile lì (e non lo è nemmeno qui), solo `article` lo è.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct FundSfdrClassification { pub fund: String, pub article: Promised<SfdrArticle> }
//! impl FundSfdrClassification {
//!     pub fn build(fund: impl Into<String>, article: &BlockValue) -> Result<Self, OutputClassError>;
//! }
//! impl PromisableFields for FundSfdrClassification { /* pending() -> ["article"] se pendente */ }
//! ```
//!
//! Deriva `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.

use serde::{Deserialize, Serialize};

use crate::commons::consts::SfdrArticle;
use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{OutputClassError, pending_of, promised_from_value, serde_promised};

/// La classificazione SFDR (art. 6/8/9) dichiarata di un fondo. `fund` non è mai promettibile,
/// solo `article` lo è.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundSfdrClassification {
    pub fund: String,
    #[serde(with = "serde_promised")]
    pub article: Promised<SfdrArticle>,
}

impl FundSfdrClassification {
    pub fn build(fund: impl Into<String>, article: &BlockValue) -> Result<Self, OutputClassError> {
        let article = promised_from_value("article", article, |v| v.sfdr_article_or_fail("article"))?;
        Ok(Self { fund: fund.into(), article })
    }
}

impl PromisableFields for FundSfdrClassification {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        pending_of("article", &self.article).into_iter().collect()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            "article" => {
                self.article = Promised::Resolved(value.sfdr_article_or_fail("article")?);
                Ok(())
            }
            other => unreachable!("FundSfdrClassification has no promisable field {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    mod construction {
        use super::*;

        #[test]
        fn builds_a_classification_from_a_resolved_article() {
            let classification =
                FundSfdrClassification::build("Alpha Fund", &BlockValue::from(SfdrArticle::Art8)).unwrap();
            assert_eq!(classification.fund, "Alpha Fund");
            assert_eq!(classification.article.resolved(), Some(&SfdrArticle::Art8));
        }

        #[test]
        fn every_sfdr_article_variant_is_accepted() {
            for article in [SfdrArticle::Art6, SfdrArticle::Art8, SfdrArticle::Art9] {
                let classification =
                    FundSfdrClassification::build("X", &BlockValue::from(article)).unwrap();
                assert_eq!(classification.article.resolved(), Some(&article));
            }
        }

        #[test]
        fn a_wrongly_typed_article_is_a_field_error_naming_the_field() {
            let err = FundSfdrClassification::build("X", &BlockValue::from("Art. 8")).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "article", .. }));
        }

        #[test]
        fn a_null_article_is_rejected_rather_than_silently_accepted() {
            assert!(FundSfdrClassification::build("X", &BlockValue::Null).is_err());
        }
    }

    mod promises {
        use super::*;
        use crate::core::promise::Promise as P;

        fn promised_classification() -> FundSfdrClassification {
            FundSfdrClassification::build("X", &BlockValue::Promise(P::new("article-id"))).unwrap()
        }

        #[test]
        fn a_promise_stays_pending_instead_of_becoming_an_article() {
            let classification = promised_classification();
            assert!(classification.article.resolved().is_none());
            assert!(classification.article.pending().is_some());
        }

        #[test]
        fn a_pending_classification_reports_its_article_field_as_pending() {
            let pending = promised_classification().pending();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].0, "article");
        }

        #[test]
        fn a_resolved_classification_reports_nothing_pending() {
            let classification = FundSfdrClassification::build("X", &BlockValue::from(SfdrArticle::Art6)).unwrap();
            assert!(classification.pending().is_empty());
        }

        #[test]
        fn resolving_the_article_field_works_in_place() {
            let mut classification = promised_classification();
            classification.resolve_field("article", BlockValue::from(SfdrArticle::Art9)).unwrap();
            assert_eq!(classification.article.resolved(), Some(&SfdrArticle::Art9));
        }

        #[test]
        fn resolving_with_a_wrongly_typed_value_is_an_error() {
            let mut classification = promised_classification();
            assert!(classification.resolve_field("article", BlockValue::from("Art. 9")).is_err());
        }

        #[test]
        fn fulfilling_against_a_map_resolves_the_article_field() {
            let mut classification = promised_classification();
            let map = FlatPromiseMap::from_iter([("article-id".to_string(), BlockValue::from(SfdrArticle::Art9))]);
            assert_eq!(fulfill_promises(&mut classification, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(classification.article.resolved(), Some(&SfdrArticle::Art9));
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_classification_survives_a_json_roundtrip() {
            let classification = FundSfdrClassification::build("X", &BlockValue::from(SfdrArticle::Art6)).unwrap();
            let json = serde_json::to_string(&classification).unwrap();
            assert_eq!(serde_json::from_str::<FundSfdrClassification>(&json).unwrap(), classification);
        }
    }
}
