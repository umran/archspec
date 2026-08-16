use serde::{Deserialize, Serialize};

use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestInput {
    pub schema: Id,
}
