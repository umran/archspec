use std::collections::BTreeMap;

use super::{DataModel, Id, Operation, Schema, Service, Topic};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub revision: Revision,

    pub services: BTreeMap<Id, Service>,
    pub schemas: BTreeMap<Id, Schema>,
    pub data_models: BTreeMap<Id, DataModel>,
    pub topics: BTreeMap<Id, Topic>,
    pub operations: BTreeMap<Id, Operation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(pub u64);
