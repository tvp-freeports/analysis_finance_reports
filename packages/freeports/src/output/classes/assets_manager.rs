//! [`ManagementCompany`] and [`InvestmentsManager`]: who runs a fund, by name, with the set of
//! funds they run.
//!
//! A shared data struct and two wrappers, as with the investment entities. They are two types
//! rather than two variants because everything downstream tells them apart by type.
//!
//! **No field is ever promisable** here, so promise fulfilment is a pure no-op: nothing is ever
//! pending and no field can be resolved.
//!
//! The managed funds are the names **as written**, unnormalised, which is the form the filtering
//! pipes already produce and the form that belongs in an output file meant to be read.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::PromisableFields;
use crate::core::promise::Promise;

use super::OutputClassError;

/// The fields a management company and an investments manager have in common: none of them is ever
/// promisable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetsManagerData {
    pub name: String,
    pub managed_funds: BTreeSet<String>,
}

impl AssetsManagerData {
    pub fn build(name: &BlockValue, managed_funds: &BlockValue) -> Result<Self, OutputClassError> {
        let name = name.str_or_fail("name").map_err(|source| OutputClassError::Field { field: "name", source })?;
        let managed_funds = managed_funds
            .set_or_fail("managed_funds")
            .map_err(|source| OutputClassError::Field { field: "managed_funds", source })?
            .iter()
            .map(|v| {
                v.str_or_fail("managed_funds")
                    .map(str::to_string)
                    .map_err(|source| OutputClassError::Field { field: "managed_funds", source })
            })
            .collect::<Result<BTreeSet<String>, _>>()?;
        Ok(Self { name: name.to_string(), managed_funds })
    }
}

/// A fund's management company.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagementCompany {
    #[serde(flatten)]
    pub data: AssetsManagerData,
}

impl ManagementCompany {
    pub fn build(name: &BlockValue, managed_funds: &BlockValue) -> Result<Self, OutputClassError> {
        Ok(Self { data: AssetsManagerData::build(name, managed_funds)? })
    }
}

impl PromisableFields for ManagementCompany {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        Vec::new()
    }

    fn resolve_field(&mut self, field: &'static str, _value: BlockValue) -> Result<(), BlockValueError> {
        unreachable!("ManagementCompany has no promisable field {field:?}")
    }
}

/// A fund's investments manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestmentsManager {
    #[serde(flatten)]
    pub data: AssetsManagerData,
}

impl InvestmentsManager {
    pub fn build(name: &BlockValue, managed_funds: &BlockValue) -> Result<Self, OutputClassError> {
        Ok(Self { data: AssetsManagerData::build(name, managed_funds)? })
    }
}

impl PromisableFields for InvestmentsManager {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        Vec::new()
    }

    fn resolve_field(&mut self, field: &'static str, _value: BlockValue) -> Result<(), BlockValueError> {
        unreachable!("InvestmentsManager has no promisable field {field:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    fn funds_value(names: &[&str]) -> BlockValue {
        BlockValue::Set(names.iter().map(|n| BlockValue::from(*n)).collect())
    }

    mod construction {
        use super::*;

        #[test]
        fn builds_a_management_company_from_name_and_funds() {
            let mc = ManagementCompany::build(&BlockValue::from("Acme AM"), &funds_value(&["Fund A", "Fund B"]))
                .unwrap();
            assert_eq!(mc.data.name, "Acme AM");
            assert_eq!(
                mc.data.managed_funds,
                BTreeSet::from(["Fund A".to_string(), "Fund B".to_string()])
            );
        }

        #[test]
        fn builds_an_investments_manager_the_same_way() {
            let im =
                InvestmentsManager::build(&BlockValue::from("Acme IM"), &funds_value(&["Fund A"])).unwrap();
            assert_eq!(im.data.name, "Acme IM");
            assert_eq!(im.data.managed_funds, BTreeSet::from(["Fund A".to_string()]));
        }

        #[test]
        fn an_empty_set_of_funds_produces_an_empty_btreeset_not_an_error() {
            let mc = ManagementCompany::build(&BlockValue::from("Acme AM"), &funds_value(&[])).unwrap();
            assert!(mc.data.managed_funds.is_empty());
        }

        #[test]
        fn unicode_fund_names_are_kept_as_written_not_normalized() {
            let mc = ManagementCompany::build(
                &BlockValue::from("Acme AM"),
                &funds_value(&["Café Balanced Fund", "Ómega Bond Fund"]),
            )
            .unwrap();
            assert_eq!(
                mc.data.managed_funds,
                BTreeSet::from(["Café Balanced Fund".to_string(), "Ómega Bond Fund".to_string()])
            );
        }

        #[test]
        fn a_wrongly_typed_name_is_a_field_error_naming_the_field() {
            let err = ManagementCompany::build(&BlockValue::from(1i64), &funds_value(&[])).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "name", .. }));
        }

        #[test]
        fn a_wrongly_typed_managed_funds_is_a_field_error_naming_the_field() {
            let err =
                ManagementCompany::build(&BlockValue::from("Acme AM"), &BlockValue::from("not a set"))
                    .unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "managed_funds", .. }));
        }

        #[test]
        fn a_managed_fund_that_is_not_a_string_is_a_field_error() {
            let bad_set = BlockValue::Set(BTreeSet::from([BlockValue::Int(1)]));
            let err = ManagementCompany::build(&BlockValue::from("Acme AM"), &bad_set).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "managed_funds", .. }));
        }
    }

    mod assets_manager_specific {
        use super::*;

        /// Built from the same name and fund set but of different types, so they are not
        /// interchangeable. The compiler guarantees it; this documents it.
        #[test]
        fn management_company_and_investments_manager_are_distinct_types_even_with_identical_data() {
            use std::any::TypeId;
            let mc = ManagementCompany::build(&BlockValue::from("Acme"), &funds_value(&["Fund A"])).unwrap();
            let im = InvestmentsManager::build(&BlockValue::from("Acme"), &funds_value(&["Fund A"])).unwrap();
            assert_eq!(mc.data, im.data, "the shared data is identical by construction");
            assert_ne!(TypeId::of::<ManagementCompany>(), TypeId::of::<InvestmentsManager>());
        }
    }

    mod promises {
        use super::*;

        #[test]
        fn pending_is_always_empty_for_a_management_company() {
            let mc = ManagementCompany::build(&BlockValue::from("Acme"), &funds_value(&["Fund A"])).unwrap();
            assert!(mc.pending().is_empty());
        }

        #[test]
        fn pending_is_always_empty_for_an_investments_manager() {
            let im = InvestmentsManager::build(&BlockValue::from("Acme"), &funds_value(&[])).unwrap();
            assert!(im.pending().is_empty());
        }

        #[test]
        fn fulfilling_a_management_company_against_any_map_is_always_in_place() {
            let mut mc = ManagementCompany::build(&BlockValue::from("Acme"), &funds_value(&["Fund A"])).unwrap();
            let before = mc.clone();
            let map = FlatPromiseMap::from_pairs([("whatever".to_string(), BlockValue::from(1i64))]);
            assert_eq!(fulfill_promises(&mut mc, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(mc, before, "nothing should have changed");
        }

        #[test]
        fn fulfilling_against_an_empty_map_is_also_always_in_place() {
            let mut im = InvestmentsManager::build(&BlockValue::from("Acme"), &funds_value(&[])).unwrap();
            assert_eq!(fulfill_promises(&mut im, &FlatPromiseMap::new()).unwrap(), Fulfilled::InPlace);
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_management_company_survives_a_json_roundtrip() {
            let mc = ManagementCompany::build(&BlockValue::from("Acme"), &funds_value(&["Fund A", "Fund B"]))
                .unwrap();
            let json = serde_json::to_string(&mc).unwrap();
            assert_eq!(serde_json::from_str::<ManagementCompany>(&json).unwrap(), mc);
        }

        #[test]
        fn an_investments_manager_survives_a_json_roundtrip() {
            let im = InvestmentsManager::build(&BlockValue::from("Acme"), &funds_value(&["Fund A"])).unwrap();
            let json = serde_json::to_string(&im).unwrap();
            assert_eq!(serde_json::from_str::<InvestmentsManager>(&json).unwrap(), im);
        }

        #[test]
        fn the_shared_fields_are_flattened_not_nested_under_a_data_key() {
            let mc = ManagementCompany::build(&BlockValue::from("Acme"), &funds_value(&[])).unwrap();
            let json = serde_json::to_string(&mc).unwrap();
            assert!(json.contains("\"name\""), "{json}");
            assert!(!json.contains("\"data\""), "{json}");
        }
    }
}
