use std::fmt;

use serde::de::value::MapAccessDeserializer;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::spec::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueRef {
    pub source: ValueSource,
    pub path: FieldPath,
}

/// Where a value comes from.
///
/// The canonical form is the tagged map. A `kind:id` string says the
/// same thing:
///
/// ```yaml
/// source: input:input.create_order.request
///
/// source:
///   kind: input
///   id: input.create_order.request
/// ```
///
/// The kind is never inferred from the id. The five variants index
/// five separate namespaces, and inferring would let the meaning of a
/// declaration depend on what happens to resolve — silently choosing
/// for an id declared in two of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ValueSource {
    Input(Id),

    /// A field in the payload of a Publication or Request effect.
    Effect(Id),

    /// A field in a logical invocation result available to the
    /// current invocation.
    InvocationResult(Id),

    /// A field on the persistent object governed by a state machine.
    StateMachineSubject(Id),

    /// A field observed by a named Read earlier in the same
    /// transaction execution.
    ///
    /// Transaction-read results are transaction-local. They do not
    /// become durable cross-transaction artifacts.
    TransactionRead(Id),
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    rename = "ValueSource"
)]
enum ValueSourceLong {
    Input(Id),
    Effect(Id),
    InvocationResult(Id),
    StateMachineSubject(Id),
    TransactionRead(Id),
}

impl From<ValueSourceLong> for ValueSource {
    fn from(long: ValueSourceLong) -> Self {
        match long {
            ValueSourceLong::Input(id) => ValueSource::Input(id),
            ValueSourceLong::Effect(id) => ValueSource::Effect(id),
            ValueSourceLong::InvocationResult(id) => ValueSource::InvocationResult(id),
            ValueSourceLong::StateMachineSubject(id) => ValueSource::StateMachineSubject(id),
            ValueSourceLong::TransactionRead(id) => ValueSource::TransactionRead(id),
        }
    }
}

/// The shorthand names, in declaration order, for an error to list.
const VALUE_SOURCE_KINDS: &str = "`input`, `effect`, `invocation_result`, \
                                  `state_machine_subject`, `transaction_read`";

/// Whether a shorthand string opens with a value source kind.
///
/// Used where a string means something else — a literal — to reject
/// what is almost certainly a value reference missing its path,
/// rather than read it as the text it happens to spell.
pub(crate) fn opens_with_value_source_kind(text: &str) -> bool {
    text.split_once(':').is_some_and(|(kind, _)| {
        matches!(
            kind.trim(),
            "input" | "effect" | "invocation_result" | "state_machine_subject" | "transaction_read"
        )
    })
}

impl<'de> Deserialize<'de> for ValueSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueSourceVisitor)
    }
}

struct ValueSourceVisitor;

impl<'de> Visitor<'de> for ValueSourceVisitor {
    type Value = ValueSource;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "a value source: a `kind:id` string such as \
             `input:input.create_order.request`, or a map with `kind` and `id`",
        )
    }

    fn visit_str<E>(self, text: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let text = text.trim();

        let Some((kind, id)) = text.split_once(':') else {
            return Err(E::custom(format!(
                "`{text}`: a value source names its kind, as in `input:{text}`"
            )));
        };

        let id = Id(id.trim().to_string());

        if id.0.is_empty() {
            return Err(E::custom(format!("`{text}`: expected an id after `{kind}:`")));
        }

        Ok(match kind.trim() {
            "input" => ValueSource::Input(id),
            "effect" => ValueSource::Effect(id),
            "invocation_result" => ValueSource::InvocationResult(id),
            "state_machine_subject" => ValueSource::StateMachineSubject(id),
            "transaction_read" => ValueSource::TransactionRead(id),
            other => {
                return Err(E::custom(format!(
                    "`{other}` is not a value source kind; expected one of {VALUE_SOURCE_KINDS}"
                )));
            }
        })
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        ValueSourceLong::deserialize(MapAccessDeserializer::new(map)).map(ValueSource::from)
    }
}

/// Provenance of an opaque value computation.
///
/// A derivation describes how values are produced. It is deliberately
/// not an expression language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Derivation {
    /// The model provides no fact about how the values are produced.
    Unspecified,

    /// The produced values are a deterministic function solely of the
    /// declared source values.
    ///
    /// This does not assert that those sources are stable across
    /// retries; replay stability of the provenance roots is
    /// established separately.
    Deterministic { from: Vec<ValueRef> },
}
