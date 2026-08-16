use serde::{Deserialize, Serialize};

use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationFlow {
    pub steps: Vec<FlowStep>,

    /// Terminal response for this flow.
    /// None is natural for subscription-driven operations.
    pub response: Option<Id>,
}

// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// #[serde(tag = "kind", content = "value", rename_all = "snake_case")]
// pub enum FlowStep {
//     Transaction(Id),

//     /// Direct attempt of an effect with no implication that
//     /// durable intent was previously established.
//     ExecuteEffect(ExecuteEffect),

//     /// Attempts to discharge an existing durable effect intent.
//     ExecuteEffectIntent(ExecuteEffectIntent),
// }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlowStep {
    Transaction { transaction: Id },

    ExecuteEffect { effect: Id },

    ExecuteEffectIntent { intent: Id },
}

// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// #[serde(deny_unknown_fields)]
// pub struct ExecuteEffect {
//     pub effect: Id,
// }

// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// #[serde(deny_unknown_fields)]
// pub struct ExecuteEffectIntent {
//     pub intent: Id,
// }
