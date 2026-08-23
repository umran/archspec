//! The trigger graph: which modeled operations an effect execution
//! goes on to invoke.
//!
//! A request names its target directly. A publication reaches every
//! subscription on its topic whose message selection admits the
//! published schema — the model's closed world of consumers. Verifiers
//! that follow effects across operations — idempotency's cascade
//! today; ordering's precedence source and process completion when
//! they come — resolve those edges here, once per model, so they all
//! agree on what "downstream" means.

use std::collections::BTreeMap;

use crate::spec::{
    Effect, Id, Input, MessageSelector, Model, Operation, PublicationEffect, SubscriptionInput,
    TransitionSideEffect, ValueSource,
};

/// A modeled consumer of messages on a topic: the operation and the
/// subscription input through which it consumes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Consumer<'a> {
    pub operation: &'a Id,
    pub input: &'a Id,
    pub subscription: &'a SubscriptionInput,
}

/// A modeled producer of messages on a topic: a publication effect and
/// the declaration that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Producer<'a> {
    pub site: ProducerSite<'a>,
    pub effect: &'a Id,
    pub publication: &'a PublicationEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProducerSite<'a> {
    /// An effect declared by an operation.
    Operation { operation: &'a Id },

    /// A side effect owned by a state-machine transition (§22).
    Transition { machine: &'a Id, transition: &'a Id },
}

#[derive(Debug)]
pub struct TriggerGraph<'a> {
    model: &'a Model,

    /// Subscriptions per topic, in model order.
    subscriptions: BTreeMap<&'a Id, Vec<Consumer<'a>>>,

    /// Publications per topic, in model order: operation effects, then
    /// transition side effects.
    publications: BTreeMap<&'a Id, Vec<Producer<'a>>>,
}

impl<'a> TriggerGraph<'a> {
    pub fn new(model: &'a Model) -> Self {
        let mut subscriptions: BTreeMap<&'a Id, Vec<Consumer<'a>>> = BTreeMap::new();

        for (operation, declaration) in &model.operations {
            for (input, declared) in &declaration.inputs {
                if let Input::Subscription(subscription) = declared {
                    subscriptions
                        .entry(&subscription.topic)
                        .or_default()
                        .push(Consumer {
                            operation,
                            input,
                            subscription,
                        });
                }
            }
        }

        let mut publications: BTreeMap<&'a Id, Vec<Producer<'a>>> = BTreeMap::new();

        for (operation, declaration) in &model.operations {
            for (effect, declared) in &declaration.effects {
                if let Effect::Publication(publication) = declared {
                    publications
                        .entry(&publication.topic)
                        .or_default()
                        .push(Producer {
                            site: ProducerSite::Operation { operation },
                            effect,
                            publication,
                        });
                }
            }
        }

        for (machine, declaration) in &model.state_machines {
            for (transition, declared) in &declaration.transitions {
                for (effect, side_effect) in &declared.side_effects {
                    if let TransitionSideEffect::Publication(publication) = side_effect {
                        publications
                            .entry(&publication.topic)
                            .or_default()
                            .push(Producer {
                                site: ProducerSite::Transition {
                                    machine,
                                    transition,
                                },
                                effect,
                                publication,
                            });
                    }
                }
            }
        }

        Self {
            model,
            subscriptions,
            publications,
        }
    }

    /// The modeled producers of `schema` on `topic`, in model order.
    pub fn producers(&self, topic: &Id, schema: &Id) -> Vec<Producer<'a>> {
        self.publications
            .get(topic)
            .into_iter()
            .flatten()
            .filter(|producer| &producer.publication.schema == schema)
            .copied()
            .collect()
    }

    /// The modeled consumers of `schema` published to `topic`, in
    /// model order: every subscription on the topic whose message
    /// selection admits the schema. `All` admits what the topic
    /// declares.
    pub fn consumers(&self, topic: &Id, schema: &Id) -> Vec<Consumer<'a>> {
        let declared = self
            .model
            .topics
            .get(topic)
            .is_some_and(|topic| topic.messages.contains(schema));

        self.subscriptions
            .get(topic)
            .into_iter()
            .flatten()
            .filter(|consumer| match &consumer.subscription.messages {
                MessageSelector::All => declared,
                MessageSelector::Only(schemas) => schemas.contains(schema),
            })
            .copied()
            .collect()
    }
}

/// Whether `operation` declares an idempotency requirement keyed
/// entirely from `input` — the declaration whose proof collapses
/// payload-equal invocations through that input into the work of one
/// logical invocation (a governing key names one input, §12).
pub fn collapses_duplicates(operation: &Operation, input: &Id) -> bool {
    operation.requirements.idempotency.iter().any(|requirement| {
        !requirement.key.components.is_empty()
            && requirement
                .key
                .components
                .iter()
                .all(|component| component.source == ValueSource::Input(input.clone()))
    })
}
