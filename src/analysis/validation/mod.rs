pub mod error;
pub mod id_declaration;
pub mod reference;

use std::collections::BTreeMap;

use crate::spec::*;

pub use error::*;
pub use id_declaration::*;
pub use reference::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Request,
    Subscription,
}

impl std::fmt::Display for InputKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request => f.write_str("request input"),
            Self::Subscription => f.write_str("subscription input"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ReferenceInfo<'a> {
    kind: ReferenceKind,
    owner: Option<&'a Id>,
}

struct ReferenceIndex<'a> {
    entries: BTreeMap<Id, ReferenceInfo<'a>>,
}

impl<'a> ReferenceIndex<'a> {
    fn build(model: &'a Model) -> Self {
        let mut entries = BTreeMap::new();

        visit_declarations(model, |id, kind, owner| {
            entries.insert(id.clone(), ReferenceInfo { kind, owner });
        });

        Self { entries }
    }

    fn get(&self, id: &Id) -> Option<ReferenceInfo<'a>> {
        self.entries.get(id).copied()
    }
}

pub fn validate(model: &Model) -> Vec<ValidationError> {
    // 1. Establish an unambiguous global namespace.
    let errors = validate_global_id_uniqueness(model);

    if !errors.is_empty() {
        return errors;
    }

    // 2. Build an index over the now-unambiguous namespace.
    let index = ReferenceIndex::build(model);

    // 3. Ensure all references resolve to valid entity kinds
    //    with the required ownership relationships.
    // let errors = validate_references(model, &index);

    // if !errors.is_empty() {
    //     return errors;
    // }

    // // 4. Ensure schema derivation itself is well-founded.
    // let errors = validate_fragment_cycles(model);

    // if !errors.is_empty() {
    //     return errors;
    // }

    // // 5. Validate structural invariants that rely on valid refs.
    // let mut errors = Vec::new();

    // errors.extend(validate_data_models(model));
    // errors.extend(validate_topics(model));
    // errors.extend(validate_state_machines(model, &index));
    // errors.extend(validate_transactions(model, &index));
    // errors.extend(validate_responses(model));
    // errors.extend(validate_field_paths(model, &index));

    errors
}

pub fn validate_global_id_uniqueness(model: &Model) -> Vec<ValidationError> {
    let mut seen: BTreeMap<&Id, IdDeclaration> = BTreeMap::new();

    let mut errors = Vec::new();

    visit_declarations(model, |id, kind, owner| {
        let declaration = IdDeclaration {
            kind,
            owner: owner.cloned(),
        };

        if let Some(first) = seen.get(id) {
            errors.push(ValidationError::DuplicateId {
                id: id.clone(),
                first: first.clone(),
                second: declaration,
            });

            return;
        }

        seen.insert(id, declaration);
    });

    errors
}

fn visit_declarations<'a>(
    model: &'a Model,
    mut visit: impl FnMut(&'a Id, ReferenceKind, Option<&'a Id>),
) {
    for id in model.services.keys() {
        visit(id, ReferenceKind::Service, None);
    }

    for id in model.schemas.keys() {
        visit(id, ReferenceKind::Schema, None);
    }

    for (data_model_id, data_model) in &model.data_models {
        visit(data_model_id, ReferenceKind::DataModel, None);

        for object_id in data_model.objects.keys() {
            visit(object_id, ReferenceKind::DataObject, Some(data_model_id));
        }
    }

    for id in model.topics.keys() {
        visit(id, ReferenceKind::Topic, None);
    }

    for (machine_id, machine) in &model.state_machines {
        visit(machine_id, ReferenceKind::StateMachine, None);

        for state_id in &machine.states {
            visit(state_id, ReferenceKind::State, Some(machine_id));
        }

        for (transition_id, transition) in &machine.transitions {
            visit(transition_id, ReferenceKind::Transition, Some(machine_id));

            for effect_id in transition.side_effects.keys() {
                visit(effect_id, ReferenceKind::Effect, Some(transition_id));
            }
        }
    }

    for (operation_id, operation) in &model.operations {
        visit(operation_id, ReferenceKind::Operation, None);

        for input_id in operation.inputs.keys() {
            visit(input_id, ReferenceKind::Input, Some(operation_id));
        }

        for effect_id in operation.effects.keys() {
            visit(effect_id, ReferenceKind::Effect, Some(operation_id));
        }

        for intent_id in operation.effect_intents.keys() {
            visit(intent_id, ReferenceKind::EffectIntent, Some(operation_id));
        }

        for result_id in operation.invocation_results.keys() {
            visit(
                result_id,
                ReferenceKind::InvocationResult,
                Some(operation_id),
            );
        }

        for response_id in operation.responses.keys() {
            visit(response_id, ReferenceKind::Response, Some(operation_id));
        }

        for transaction_id in operation.transactions.keys() {
            visit(
                transaction_id,
                ReferenceKind::Transaction,
                Some(operation_id),
            );
        }

        for flow_id in operation.flows.keys() {
            visit(flow_id, ReferenceKind::InvocationFlow, Some(operation_id));
        }
    }
}

fn expect_owned_by(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    reference: &Id,
    expected_owner: &Id,
    errors: &mut Vec<ValidationError>,
) -> bool {
    let Some(info) = index.get(reference) else {
        return false;
    };

    if info.owner != Some(expected_owner) {
        errors.push(ValidationError::InvalidReferenceOwner {
            subject: subject.clone(),
            reference: reference.clone(),
            expected_owner: expected_owner.clone(),
            actual_owner: info.owner.cloned(),
        });

        return false;
    }

    true
}

fn expect_reference(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    reference: &Id,
    expected: ReferenceKind,
    errors: &mut Vec<ValidationError>,
) -> bool {
    let Some(info) = index.get(reference) else {
        errors.push(ValidationError::UnknownReference {
            subject: subject.clone(),
            reference: reference.clone(),
            expected,
        });

        return false;
    };

    if info.kind != expected {
        errors.push(ValidationError::InvalidReferenceKind {
            subject: subject.clone(),
            reference: reference.clone(),
            expected,
            actual: info.kind,
        });

        return false;
    }

    true
}
