use serde::{Deserialize, Serialize};

use crate::spec::Id;

/// A logical transaction artifact describing an intended effect
/// execution.
///
/// An effect intent is not inherently a durable record and does not
/// imply an independent executor. `ExecuteEffectIntent` is the modeled
/// execution authority; establishment alone does not execute the
/// underlying effect.
///
/// An intent whose effect is owned by a state-machine transition is
/// established implicitly by a successful transition, rather than by
/// an explicit `EstablishEffectIntent` step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectIntent {
    pub effect: Id,
}
