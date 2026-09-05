//! [`FundRename`] and [`FundMerge`]: a fund's change-of-name event.
//!
//! A shared data struct carrying all the promise handling, and two wrappers that are not variants
//! of one enum because everything downstream tells them apart by **type** — they end up on two
//! different paths — never by matching on a field.
//!
//! No numeric constraints here: the two names are free strings and the date is the only promisable
//! field.

use serde::{Deserialize, Serialize};

use crate::commons::date::Date;
use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{OutputClassError, pending_of, promised_from_value};

/// The fields a rename and a merge have in common: the name before and after the event, and the
/// date it happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundChangeNameData {
    pub old_name: String,
    pub current_name: String,
    pub date: Promised<Date>,
}

impl FundChangeNameData {
    pub fn build(
        old_name: impl Into<String>,
        current_name: impl Into<String>,
        date: &BlockValue,
    ) -> Result<Self, OutputClassError> {
        let date = promised_from_value("date", date, |v| v.date_or_fail("date"))?;
        Ok(Self { old_name: old_name.into(), current_name: current_name.into(), date })
    }

    fn pending_fields(&self) -> Vec<(&'static str, Promise)> {
        pending_of("date", &self.date).into_iter().collect()
    }

    fn resolve(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            "date" => {
                self.date = Promised::Resolved(value.date_or_fail("date")?);
                Ok(())
            }
            other => unreachable!("FundChangeNameData has no promisable field {other:?}"),
        }
    }
}

/// The renaming of a fund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundRename {
    #[serde(flatten)]
    pub data: FundChangeNameData,
}

impl FundRename {
    pub fn build(
        old_name: impl Into<String>,
        current_name: impl Into<String>,
        date: &BlockValue,
    ) -> Result<Self, OutputClassError> {
        Ok(Self { data: FundChangeNameData::build(old_name, current_name, date)? })
    }
}

impl PromisableFields for FundRename {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        self.data.pending_fields()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        self.data.resolve(field, value)
    }
}

/// The merger of one fund into another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundMerge {
    #[serde(flatten)]
    pub data: FundChangeNameData,
}

impl FundMerge {
    pub fn build(
        old_name: impl Into<String>,
        current_name: impl Into<String>,
        date: &BlockValue,
    ) -> Result<Self, OutputClassError> {
        Ok(Self { data: FundChangeNameData::build(old_name, current_name, date)? })
    }
}

impl PromisableFields for FundMerge {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        self.data.pending_fields()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        self.data.resolve(field, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    fn resolved_date() -> Date {
        Date::new(2025, 7, 2).unwrap()
    }

    mod construction {
        use super::*;

        #[test]
        fn builds_a_fund_rename_with_all_fields_resolved() {
            let rename =
                FundRename::build("Old Fund", "New Fund", &BlockValue::from(resolved_date())).unwrap();
            assert_eq!(rename.data.old_name, "Old Fund");
            assert_eq!(rename.data.current_name, "New Fund");
            assert_eq!(rename.data.date.resolved(), Some(&resolved_date()));
        }

        #[test]
        fn builds_a_fund_merge_the_same_way() {
            let merge =
                FundMerge::build("Old Fund", "New Fund", &BlockValue::from(resolved_date())).unwrap();
            assert_eq!(merge.data.old_name, "Old Fund");
            assert_eq!(merge.data.current_name, "New Fund");
        }

        #[test]
        fn a_wrongly_typed_date_is_a_field_error_naming_the_field() {
            let err = FundRename::build("Old", "New", &BlockValue::from(1i64)).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "date", .. }));
        }

        #[test]
        fn a_null_date_is_rejected_rather_than_silently_accepted() {
            assert!(FundRename::build("Old", "New", &BlockValue::Null).is_err());
        }
    }

    mod promises {
        use super::*;
        use crate::core::promise::Promise as P;

        fn promised_rename() -> FundRename {
            FundRename::build("Old", "New", &BlockValue::Promise(P::new("date-id"))).unwrap()
        }

        #[test]
        fn a_pending_date_reports_its_field_as_pending() {
            let pending = promised_rename().pending();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].0, "date");
        }

        #[test]
        fn a_fully_resolved_rename_reports_nothing_pending() {
            let rename = FundRename::build("Old", "New", &BlockValue::from(resolved_date())).unwrap();
            assert!(rename.pending().is_empty());
        }

        #[test]
        fn resolving_the_date_field_works_in_place() {
            let mut rename = promised_rename();
            rename.resolve_field("date", BlockValue::from(resolved_date())).unwrap();
            assert_eq!(rename.data.date.resolved(), Some(&resolved_date()));
        }

        #[test]
        fn resolving_with_a_wrongly_typed_value_reports_the_field() {
            let mut rename = promised_rename();
            let err = rename.resolve_field("date", BlockValue::from(1i64)).unwrap_err();
            assert!(err.to_string().contains("date"), "{err}");
        }

        #[test]
        fn fulfilling_against_a_map_produces_the_same_rename_as_direct_construction() {
            let mut rename = promised_rename();
            let map = FlatPromiseMap::from_pairs([("date-id".to_string(), BlockValue::from(resolved_date()))]);
            assert_eq!(fulfill_promises(&mut rename, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(rename, FundRename::build("Old", "New", &BlockValue::from(resolved_date())).unwrap());
        }

        #[test]
        fn a_merge_resolves_the_same_shared_field_as_a_rename() {
            let mut merge = FundMerge::build("Old", "New", &BlockValue::Promise(P::new("date-id"))).unwrap();
            let map = FlatPromiseMap::from_pairs([("date-id".to_string(), BlockValue::from(resolved_date()))]);
            assert_eq!(fulfill_promises(&mut merge, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(merge.data.date.resolved(), Some(&resolved_date()));
        }

        #[test]
        fn a_non_strict_unresolvable_date_drops_the_entity() {
            let mut rename = promised_rename();
            assert_eq!(fulfill_promises(&mut rename, &FlatPromiseMap::new()).unwrap(), Fulfilled::Dropped);
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_fund_rename_survives_a_json_roundtrip() {
            let rename = FundRename::build("Old", "New", &BlockValue::from(resolved_date())).unwrap();
            let json = serde_json::to_string(&rename).unwrap();
            assert_eq!(serde_json::from_str::<FundRename>(&json).unwrap(), rename);
        }

        #[test]
        fn a_resolved_fund_merge_survives_a_json_roundtrip() {
            let merge = FundMerge::build("Old", "New", &BlockValue::from(resolved_date())).unwrap();
            let json = serde_json::to_string(&merge).unwrap();
            assert_eq!(serde_json::from_str::<FundMerge>(&json).unwrap(), merge);
        }

        #[test]
        fn the_shared_fields_are_flattened_not_nested_under_a_data_key() {
            let json =
                serde_json::to_string(&FundRename::build("Old", "New", &BlockValue::from(resolved_date())).unwrap())
                    .unwrap();
            assert!(json.contains("\"old_name\""), "{json}");
            assert!(!json.contains("\"data\""), "{json}");
        }
    }

    mod fund_change_name_specific {
        use super::*;

        /// The two are not comparable to each other — a direct comparison does not even compile —
        /// so the invariant is the compiler's. This documents it by checking they really are two
        /// distinct types, even when built from the same fields.
        #[test]
        fn fund_rename_and_fund_merge_are_distinct_types_even_with_identical_fields() {
            use std::any::TypeId;
            let rename = FundRename::build("Old", "New", &BlockValue::from(resolved_date())).unwrap();
            let merge = FundMerge::build("Old", "New", &BlockValue::from(resolved_date())).unwrap();
            assert_eq!(rename.data, merge.data, "the shared data is identical by construction");
            assert_ne!(TypeId::of::<FundRename>(), TypeId::of::<FundMerge>());
        }
    }
}
