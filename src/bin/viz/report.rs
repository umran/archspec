//! Provisional prover-report format.
//!
//! The prover and model checker are not implemented yet. This module
//! infers a plausible report shape from the DSL and its semantics so
//! the presentation layer can be built incrementally: obligations are
//! discharged per declared requirement, a proof states the facts it
//! relies on, a disproof carries a counterexample trace, and `unknown`
//! is epistemic (the solver could not decide), never a violation.
//!
//! Once the real prover format is finalized this module is the single
//! place to adapt; the visualization consumes only this shape.

use serde::{Deserialize, Serialize};

use archspec::spec::{Id, Model};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProverReport {
    /// Version of this report format, not of the model.
    pub format: u32,

    /// Revision of the model the report was produced against.
    ///
    /// The visualization warns when this disagrees with the rendered
    /// model's revision.
    pub model_revision: Option<u64>,

    pub obligations: Vec<Obligation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    /// Stable identity of the obligation within the report.
    pub id: String,

    pub property: Property,
    pub subject: Subject,
    pub status: Status,

    /// One-line human-readable statement of the obligation.
    pub summary: String,

    /// Declared model facts the verdict relies on. A proof is
    /// conditional on the implementation conforming to these.
    #[serde(default)]
    pub assumptions: Vec<String>,

    /// Model facts explaining how the verdict was reached.
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,

    /// Present only when `status` is `disproven`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterexample: Option<Counterexample>,
}

/// The correctness property an obligation discharges.
///
/// The first four mirror `OperationRequirements`; `response_replay`
/// splits out the response half of an idempotency requirement;
/// `object_history` mirrors `ObjectHistoryRequirement`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Property {
    Serialization,
    Ordering,
    Idempotency,
    Recoverability,
    ResponseReplay,
    ObjectHistory,
    Custom { name: String },
}

/// The model entity an obligation is anchored to.
///
/// `requirement` indexes into the corresponding requirement list on
/// the operation, tying the obligation back to the declaration that
/// produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Subject {
    Operation {
        operation: Id,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        requirement: Option<usize>,
    },
    Flow {
        operation: Id,
        flow: Id,
    },
    Transaction {
        operation: Id,
        transaction: Id,
    },
    Object {
        data_model: Id,
        object: Id,
    },
    StateMachine {
        machine: Id,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transition: Option<Id>,
    },
    Topic {
        topic: Id,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The property follows for all executions admitted by the model.
    Proven,

    /// The solver found an admitted execution violating the property.
    Disproven,

    /// The solver could not decide, typically because a required fact
    /// is `unspecified`. Not evidence of a violation.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceItem {
    /// Model entity the fact concerns, when one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Id>,

    pub message: String,
}

/// A concrete admitted execution that violates the property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counterexample {
    pub trace: Vec<TraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceStep {
    /// Entity performing the step (an operation, topic, or the
    /// environment), when one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Id>,

    pub description: String,
}

/// Builds a scaffold report enumerating every obligation the declared
/// requirements imply, all with status `unknown`.
///
/// Doubles as executable documentation of the format and as the shape
/// the eventual prover output is expected to fill in.
pub fn scaffold(model: &Model) -> ProverReport {
    let mut obligations = Vec::new();

    fn requirement_obligation(
        op_id: &Id,
        property: Property,
        index: usize,
        summary: String,
    ) -> Obligation {
        Obligation {
            id: format!(
                "oblig.{}.{}.{}",
                op_id,
                property_slug(&property),
                index
            ),
            property,
            subject: Subject::Operation {
                operation: op_id.clone(),
                requirement: Some(index),
            },
            status: Status::Unknown,
            summary,
            assumptions: Vec::new(),
            evidence: Vec::new(),
            counterexample: None,
        }
    }

    for (op_id, op) in &model.operations {
        let push = |obligations: &mut Vec<Obligation>,
                    property: Property,
                    index: usize,
                    summary: String| {
            obligations.push(requirement_obligation(
                op_id, property, index, summary,
            ));
        };

        for (i, r) in op.requirements.serialization.iter().enumerate() {
            push(
                &mut obligations,
                Property::Serialization,
                i,
                format!(
                    "Invocations of {op_id} sharing key {} never overlap.",
                    value_ref_label(&r.key)
                ),
            );
        }

        for (i, r) in op.requirements.ordering.iter().enumerate() {
            push(
                &mut obligations,
                Property::Ordering,
                i,
                format!(
                    "Invocations of {op_id} sharing key {} take effect in \
                     their semantic order.",
                    value_ref_label(&r.key)
                ),
            );
        }

        for (i, r) in op.requirements.idempotency.iter().enumerate() {
            push(
                &mut obligations,
                Property::Idempotency,
                i,
                format!(
                    "Repeated attempts at {op_id} sharing the declared key \
                     produce the effects of a single invocation.",
                ),
            );

            if r.response
                == archspec::spec::ResponseReplayRequirement::ReplayConsistent
            {
                obligations.push(Obligation {
                    id: format!("oblig.{op_id}.response_replay.{i}"),
                    property: Property::ResponseReplay,
                    subject: Subject::Operation {
                        operation: op_id.clone(),
                        requirement: Some(i),
                    },
                    status: Status::Unknown,
                    summary: format!(
                        "Every attempt at {op_id} sharing the declared key \
                         observes an equivalent response."
                    ),
                    assumptions: Vec::new(),
                    evidence: Vec::new(),
                    counterexample: None,
                });
            }
        }

        for (i, r) in op.requirements.recoverability.iter().enumerate() {
            push(
                &mut obligations,
                Property::Recoverability,
                i,
                format!(
                    "An interrupted invocation of {op_id} {} a declared \
                     flow's terminal step.",
                    match r.completion {
                        archspec::spec::CompletionRequirement::Resumable =>
                            "can be resumed to reach",
                        archspec::spec::CompletionRequirement::Guaranteed =>
                            "is re-driven until it reaches",
                    }
                ),
            );
        }
    }

    for (dm_id, dm) in &model.data_models {
        for (obj_id, obj) in &dm.objects {
            for req in &obj.requirements.history {
                let name = match req {
                    archspec::spec::ObjectHistoryRequirement::Linearizable => {
                        "linearizable"
                    }
                };

                obligations.push(Obligation {
                    id: format!("oblig.{dm_id}.{obj_id}.history.{name}"),
                    property: Property::ObjectHistory,
                    subject: Subject::Object {
                        data_model: dm_id.clone(),
                        object: obj_id.clone(),
                    },
                    status: Status::Unknown,
                    summary: format!(
                        "Accesses to {obj_id} admit a legal sequential \
                         history respecting real-time precedence."
                    ),
                    assumptions: Vec::new(),
                    evidence: Vec::new(),
                    counterexample: None,
                });
            }
        }
    }

    ProverReport {
        format: 1,
        model_revision: Some(model.revision.0),
        obligations,
    }
}

fn property_slug(property: &Property) -> &str {
    match property {
        Property::Serialization => "serialization",
        Property::Ordering => "ordering",
        Property::Idempotency => "idempotency",
        Property::Recoverability => "recoverability",
        Property::ResponseReplay => "response_replay",
        Property::ObjectHistory => "object_history",
        Property::Custom { name } => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flash_checkout() -> Model {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/flash_checkout.yaml");

        archspec::parser::yaml::parse(
            &std::fs::read_to_string(path).expect("fixture readable"),
        )
        .expect("fixture parses")
    }

    #[test]
    fn scaffold_enumerates_requirement_obligations() {
        let model = flash_checkout();
        let report = scaffold(&model);

        assert_eq!(report.format, 1);
        assert_eq!(report.model_revision, Some(1));

        // 3 serialization + 3 ordering + 4 idempotency + 1 response
        // replay + 3 recoverability + 2 object history.
        assert_eq!(report.obligations.len(), 16);

        assert!(report
            .obligations
            .iter()
            .all(|ob| ob.status == Status::Unknown));

        assert!(report.obligations.iter().any(|ob| matches!(
            &ob.property,
            Property::ResponseReplay
        )));
        assert_eq!(
            report
                .obligations
                .iter()
                .filter(|ob| matches!(ob.property, Property::ObjectHistory))
                .count(),
            2,
        );
    }

    #[test]
    fn scaffold_round_trips_through_json() {
        let report = scaffold(&flash_checkout());

        let json = serde_json::to_string_pretty(&report).expect("serializes");
        let parsed: ProverReport =
            serde_json::from_str(&json).expect("deserializes");

        assert_eq!(parsed, report);
    }

    #[test]
    fn demo_report_fixture_parses() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/flash_checkout.report.json");

        let raw = std::fs::read_to_string(path).expect("fixture readable");
        let report: ProverReport =
            serde_json::from_str(&raw).expect("demo report matches format");

        assert!(report
            .obligations
            .iter()
            .any(|ob| ob.status == Status::Disproven
                && ob.counterexample.is_some()));
    }
}

fn value_ref_label(value: &archspec::spec::ValueRef) -> String {
    let source = match &value.source {
        archspec::spec::ValueSource::Input(id) => id.to_string(),
        archspec::spec::ValueSource::Effect(id) => id.to_string(),
        archspec::spec::ValueSource::InvocationResult(id) => id.to_string(),
        archspec::spec::ValueSource::StateMachineSubject(id) => id.to_string(),
        archspec::spec::ValueSource::TransactionRead(id) => id.to_string(),
    };

    format!("{source}.{}", value.path)
}

