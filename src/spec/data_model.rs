use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataModel {
    /// Logical persistent objects belonging to this transactional
    /// state boundary.
    pub objects: BTreeMap<Id, DataObject>,
}

/// A logical class of persistent object instances.
///
/// Object-history requirements (a `linearizable` flag on the object)
/// are deliberately absent: Conseqa does not yet model the
/// replication, routing, and availability facts from which such a
/// requirement could be proved, so the DSL currently models transaction
/// and operation correctness without declaring end-to-end persistent
/// object history consistency. They are to be reconsidered, as a
/// coherent family, alongside a future model of distributed persistence.
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
    ///
    /// Identity is what selector precision, insert uniqueness,
    /// alias and interference analysis, locking, state-machine subject
    /// identity, and transaction reasoning rest on.
    pub identity: Vec<FieldPath>,
}
