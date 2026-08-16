use std::collections::BTreeMap;

use crate::spec::StateMachine;

use super::{DataModel, Id, Operation, Schema, Service, Topic};

pub struct Model {
    pub revision: Revision,

    pub services: BTreeMap<Id, Service>,
    pub schemas: BTreeMap<Id, Schema>,
    pub data_models: BTreeMap<Id, DataModel>,
    pub topics: BTreeMap<Id, Topic>,

    pub state_machines: BTreeMap<Id, StateMachine>,
    pub operations: BTreeMap<Id, Operation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(pub u64);
