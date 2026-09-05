use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::spec::{FieldPath, Id, PublicationEffect, RequestEffect};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateMachine {
    pub subject: StateMachineSubject,

    pub states: BTreeSet<Id>,
    pub initial: Id,

    pub transitions: BTreeMap<Id, Transition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateMachineSubject {
    Object {
        object: Id,

        /// Field on the object's canonical schema containing
        /// the machine state.
        state: FieldPath,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub from: BTreeSet<Id>,
    pub to: Id,

    pub side_effects: BTreeMap<Id, TransitionSideEffect>,
}

/// An effect associated with taking a transition.
///
/// A side effect is not executed inside the application-state
/// transaction. Each transaction application of the transition
/// supplies, for every side effect, the concrete instance derivation
/// and an operation-local intent binding; a successful transition
/// establishes the bound `EffectIntent` artifact, subject to the same
/// retention and recovery rules as an explicitly established intent.
/// The operation executes it with `ExecuteEffectIntent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionSideEffect {
    Publication(PublicationEffect),
    Request(RequestEffect),
}
