use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataModel {
    /// Logical persistent objects belonging to this transactional
    /// state boundary.
    pub objects: BTreeMap<Id, DataObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataObject {
    /// Canonical schema describing the state of this object.
    pub schema: Id,

    /// Fields defining the identity of one logical object instance.
    ///
    /// For example:
    ///
    /// Account[id]
    ///
    /// or a composite identity:
    ///
    /// TenantAccount[tenant_id, account_id]
    pub identity: Vec<FieldPath>,

    /// History-level correctness properties required of this object.
    pub requirements: ObjectRequirements,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRequirements {
    pub history: BTreeSet<ObjectHistoryRequirement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectHistoryRequirement {
    /// Operations observing or mutating instances of this object
    /// must collectively admit a legal sequential history that
    /// respects real-time precedence.
    Linearizable,
}
