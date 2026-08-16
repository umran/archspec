use serde::{Deserialize, Serialize};

use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectIntent {
    pub effect: Id,
    pub execution: IntentExecutionSemantics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentExecutionSemantics {
    /// The intent itself may be durably established, but the model
    /// provides no guarantee that abandoned pending intents will
    /// independently be rediscovered.
    Unspecified,

    /// Once established, pending work remains durably discoverable
    /// and eligible for retry independently of the invocation that
    /// created it.
    Recoverable,
}
