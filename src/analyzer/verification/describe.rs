//! Shared rendering helpers for verification diagnostics.

use crate::analyzer::Evidence;
use crate::spec::{Id, ValueRef, ValueSource};

use super::replay::{GoverningKeyDefect, PayloadIdentityGap, ReplayGap, StabilityGap};

pub(crate) fn governing_key_evidence(defect: &GoverningKeyDefect) -> Evidence {
    match defect {
        GoverningKeyDefect::Empty => Evidence {
            subject: None,
            message: "The governing key is empty: it places every attempt in one \
                      class, and essentially nothing is replay-stable relative to \
                      it."
                .to_string(),
        },

        GoverningKeyDefect::ComponentNotFromInput { source } => Evidence {
            subject: None,
            message: format!(
                "A governing-key component is sourced from {}, which cannot define \
                 a pre-execution equivalence class.",
                describe_value_source(source)
            ),
        },

        GoverningKeyDefect::ComponentsFromMultipleInputs { first, second } => Evidence {
            subject: None,
            message: format!(
                "The governing key mixes components of inputs `{first}` and \
                 `{second}`; no single triggering input defines the population."
            ),
        },

        GoverningKeyDefect::InputNotDeclared { input } => Evidence {
            subject: Some(input.clone()),
            message: format!(
                "The governing key names input `{input}`, which the operation \
                 does not declare."
            ),
        },
    }
}

pub(crate) fn gap_sentences(gaps: &[ReplayGap]) -> String {
    if gaps.is_empty() {
        return "no gaps recorded".to_string();
    }

    gaps.iter().map(gap_sentence).collect::<Vec<_>>().join("; ")
}

fn gap_sentence(gap: &ReplayGap) -> String {
    match gap {
        ReplayGap::NoKeyedCommit => {
            "the transaction declares no keyed commit deduplication".to_string()
        }

        ReplayGap::CommitKeyRootUnstable { root, gap } => format!(
            "commit-key component `{}` is not replay-stable ({})",
            root.path,
            stability_sentence(gap)
        ),

        ReplayGap::ContainsTransition => {
            "the transaction applies a state transition, which V1 never replays naturally"
                .to_string()
        }

        ReplayGap::ContainsInsert => "the transaction inserts an object, and \
                                      duplicate-identity insert outcomes are not yet \
                                      defined"
            .to_string(),

        ReplayGap::ContainsDelete => {
            "the transaction deletes objects, and deletion replay outcomes are not defined"
                .to_string()
        }

        ReplayGap::MutationTargetRootUnstable { root, gap } => format!(
            "a mutation target depends on `{}`, which is not replay-stable ({})",
            root.path,
            stability_sentence(gap)
        ),

        ReplayGap::MutationDerivationUnspecified => {
            "a mutation declares no value provenance".to_string()
        }

        ReplayGap::MutationDerivationRootUnstable { root, gap } => format!(
            "a mutation value depends on `{}`, which is not replay-stable ({})",
            root.path,
            stability_sentence(gap)
        ),

        ReplayGap::ArtifactDerivationUnspecified => {
            "the artifact's establishment declares no value provenance".to_string()
        }

        ReplayGap::ArtifactDerivationRootUnstable { root, gap } => format!(
            "the artifact's values depend on `{}`, which is not replay-stable ({})",
            root.path,
            stability_sentence(gap)
        ),
    }
}

pub(crate) fn stability_sentence(gap: &StabilityGap) -> String {
    match gap {
        StabilityGap::UnidentifiedPayloadField { input, identity } => match identity {
            PayloadIdentityGap::NotDeclared => {
                format!("no declared identity makes non-key fields of `{input}` replay-stable")
            }

            PayloadIdentityGap::SchemaNotMapped { schema } => {
                format!("the topic declares no message identity for admitted schema `{schema}`")
            }

            PayloadIdentityGap::NotPinnedByKey { schema, field } => match schema {
                Some(schema) => format!(
                    "identity field `{field}` of `{schema}` is not pinned by the \
                     governing key"
                ),

                None => format!(
                    "the declared identity field `{field}` is not pinned by the \
                     governing key"
                ),
            },
        },

        StabilityGap::NotTriggeringInput { input } => format!(
            "it belongs to input `{input}`, which does not trigger the key-bearing \
             invocations"
        ),

        StabilityGap::MutableSubjectState { machine } => {
            format!("it reads mutable state governed by `{machine}`")
        }

        StabilityGap::EffectPayloadRoot { effect } => {
            format!("effect payloads such as `{effect}` are not stable roots in V1")
        }

        StabilityGap::TransactionReadRoot { read } => {
            format!("it reaches transaction read `{read}`, which is never replay-stable")
        }

        StabilityGap::ArtifactUnavailable { artifact } => {
            format!("artifact `{artifact}` is not replay-available")
        }

        StabilityGap::ArtifactNotInContext { artifact } => {
            format!("artifact `{artifact}` is not established before this point of the flow")
        }
    }
}

pub(crate) fn describe_value_ref(value: &ValueRef) -> String {
    format!(
        "the key (`{}` of {})",
        value.path,
        describe_value_source(&value.source)
    )
}

pub(crate) fn describe_value_source(source: &ValueSource) -> String {
    match source {
        ValueSource::Input(id) => format!("input `{id}`"),
        ValueSource::Effect(id) => format!("effect `{id}`"),
        ValueSource::InvocationResult(id) => format!("invocation result `{id}`"),
        ValueSource::StateMachineSubject(id) => format!("state-machine subject `{id}`"),
        ValueSource::TransactionRead(id) => format!("transaction read `{id}`"),
    }
}

pub(crate) fn value_source_id(source: &ValueSource) -> Option<&Id> {
    match source {
        ValueSource::Input(id)
        | ValueSource::Effect(id)
        | ValueSource::InvocationResult(id)
        | ValueSource::StateMachineSubject(id)
        | ValueSource::TransactionRead(id) => Some(id),
    }
}
