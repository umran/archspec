use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::spec::StateMachine;

use super::{DataModel, Id, Operation, Schema, Service, Topic};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub revision: Revision,

    pub services: BTreeMap<Id, Service>,
    pub schemas: BTreeMap<Id, Schema>,
    pub data_models: BTreeMap<Id, DataModel>,
    pub topics: BTreeMap<Id, Topic>,

    pub state_machines: BTreeMap<Id, StateMachine>,
    pub operations: BTreeMap<Id, Operation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub u64);
