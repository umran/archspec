use std::collections::{BTreeMap, BTreeSet};

use crate::spec::{FieldPath, Id, PublicationEffect, RequestEffect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMachine {
    pub subject: StateMachineSubject,

    pub states: BTreeSet<Id>,
    pub initial: Id,

    pub transitions: BTreeMap<Id, Transition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateMachineSubject {
    Object {
        object: Id,

        /// Field on the object's canonical schema containing
        /// the machine state.
        state: FieldPath,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub from: BTreeSet<Id>,
    pub to: Id,

    pub side_effects: BTreeMap<Id, TransitionSideEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionSideEffect {
    Publication(PublicationEffect),
    Request(RequestEffect),
}
