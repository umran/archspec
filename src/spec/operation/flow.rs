use serde::{Deserialize, Serialize};

use crate::spec::Id;

use super::Derivation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationFlow {
    pub steps: Vec<FlowStep>,

    /// Terminal response for this flow.
    /// None is natural for subscription-driven operations.
    pub response: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlowStep {
    Transaction {
        transaction: Id,
    },

    ExecuteEffect {
        effect: Id,

        /// Provenance of the complete logical effect instance
        /// constructed and executed by this step.
        values: Derivation,
    },

    /// Executes an already-established effect instance; the values were
    /// fixed at establishment, so no derivation is declared here.
    ExecuteEffectIntent {
        intent: Id,
    },
}
