use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::value::{MapAccessDeserializer, StrDeserializer};
use serde::de::{self, DeserializeSeed, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::spec::operation::value::opens_with_value_source_kind;
use crate::spec::{FieldPath, Id};

use super::{Derivation, IdempotencyGuarantee, ValueRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    /// None is permitted when the transaction performs no application
    /// DataObject access and only produces or consumes framework
    /// transaction artifacts.
    pub data_model: Option<Id>,

    pub isolation: TransactionIsolation,

    /// Explicit durable keyed commit deduplication provided by the
    /// execution environment.
    ///
    /// This is independent of any transaction-output or effect-intent
    /// declaration. `Unspecified` and `NotDeduplicated` leave the
    /// analyzer free to prove natural replayability from the body.
    pub idempotency: IdempotencyGuarantee,

    pub steps: Vec<TransactionStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionIsolation {
    Unspecified,
    ReadCommitted,
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransactionStep {
    Read(Read),
    Write(Write),
    Insert(Insert),
    Delete(Delete),
    Lock(Lock),

    Transition(StateTransition),
    EstablishEffectIntent(EstablishEffectIntent),
    EstablishTransactionOutput(EstablishTransactionOutput),
}

impl TransactionStep {
    /// Every value reference the step evaluates: selector roots,
    /// mutation and artifact derivations, and transition effect values.
    /// The transaction's commit key is not a step's and is judged
    /// separately.
    pub fn roots(&self) -> Vec<&ValueRef> {
        match self {
            Self::Read(read) => read.target.predicate.roots(),

            Self::Write(write) => {
                let mut roots = write.target.predicate.roots();

                roots.extend(write.values.roots());

                roots
            }

            Self::Insert(insert) => insert.values.roots(),

            Self::Delete(delete) => delete.target.predicate.roots(),

            Self::Lock(lock) => lock.target.predicate.roots(),

            Self::Transition(transition) => {
                let mut roots = transition.subject.predicate.roots();

                for values in transition.effect_values.values() {
                    roots.extend(values.roots());
                }

                roots
            }

            Self::EstablishEffectIntent(establish) => establish.values.roots(),

            Self::EstablishTransactionOutput(establish) => establish.values.roots(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    /// Transaction-local identity of this observation.
    ///
    /// Later steps in the same transaction may reference the observed
    /// values through `ValueSource::TransactionRead`.
    pub result: Id,

    pub target: ObjectSelector,
    pub fields: FieldSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub target: ObjectSelector,
    pub fields: BTreeSet<FieldPath>,

    /// Provenance of the values written.
    pub values: Derivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Insert {
    pub object: Id,

    /// Provenance of the inserted contents.
    ///
    /// An insert never redeclares object identity: `DataObject.identity`
    /// is already the complete logical identity of every instance.
    pub values: Derivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    pub target: ObjectSelector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub target: ObjectSelector,
    pub mode: LockMode,
    pub order: LockOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "terms", rename_all = "snake_case")]
pub enum LockOrder {
    Unspecified,
    By(Vec<OrderingTerm>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderingTerm {
    pub field: FieldPath,
    pub direction: OrderingDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderingDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
pub enum FieldSelection {
    All,
    Only(BTreeSet<FieldPath>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectSelector {
    pub object: Id,
    pub predicate: SelectorPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectorPredicate {
    All,

    Eq {
        field: FieldPath,
        value: SelectorValue,
    },

    And {
        predicates: Vec<SelectorPredicate>,
    },
}

impl SelectorPredicate {
    /// Every value reference the predicate constrains against.
    /// Literals are constants and contribute nothing.
    pub fn roots(&self) -> Vec<&ValueRef> {
        match self {
            Self::All => Vec::new(),

            Self::Eq { value, .. } => match value {
                SelectorValue::Value(root) => vec![root],
                SelectorValue::Literal(_) => Vec::new(),
            },

            Self::And { predicates } => predicates
                .iter()
                .flat_map(SelectorPredicate::roots)
                .collect(),
        }
    }
}

/// What a selector compares a field against.
///
/// The canonical form is the tagged map. The two alternatives are
/// structurally disjoint, so each may also be written as itself: a
/// reference is a map, a literal is a plain scalar.
///
/// ```yaml
/// value:
///   source: input:input.transfer_stock.request
///   path: sku
///
/// value: pending
/// ```
///
/// Inferring *this* discriminant is safe where inferring a
/// `ValueSource`'s kind is not: nothing has to be resolved to tell a
/// map from a scalar, whereas the five value sources are all ids and
/// differ only in which namespace they name. §19 relies on a selector
/// exposing its literals and references structurally, and the
/// shorthand keeps that distinction visible rather than defaulting
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SelectorValue {
    Value(ValueRef),
    Literal(Literal),
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    rename = "SelectorValue"
)]
enum SelectorValueLong {
    Value(ValueRef),
    Literal(Literal),
}

impl From<SelectorValueLong> for SelectorValue {
    fn from(long: SelectorValueLong) -> Self {
        match long {
            SelectorValueLong::Value(value) => SelectorValue::Value(value),
            SelectorValueLong::Literal(literal) => SelectorValue::Literal(literal),
        }
    }
}

impl<'de> Deserialize<'de> for SelectorValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SelectorValueVisitor)
    }
}

struct SelectorValueVisitor;

impl<'de> Visitor<'de> for SelectorValueVisitor {
    type Value = SelectorValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "a selector value: a value reference with `source` and `path`, \
             a literal scalar such as `pending`, `true`, or `3`, \
             or a map with `kind` and `value`",
        )
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let Some(key) = map.next_key::<String>()? else {
            return Err(de::Error::custom(
                "an empty map is neither a value reference nor a literal",
            ));
        };

        // The key that opens the map says which form this is, and is
        // replayed so the derived reader still sees a whole map.
        match key.as_str() {
            "source" | "path" => {
                ValueRef::deserialize(MapAccessDeserializer::new(Replayed::new(key, map)))
                    .map(SelectorValue::Value)
            }
            "kind" | "value" => {
                SelectorValueLong::deserialize(MapAccessDeserializer::new(Replayed::new(key, map)))
                    .map(SelectorValue::from)
            }
            other => Err(de::Error::custom(format!(
                "unknown field `{other}`, expected `source` and `path` for a value reference, \
                 or `kind` and `value`"
            ))),
        }
    }

    fn visit_str<E>(self, text: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        // A reference that lost its path would otherwise read as the
        // string it spells, quietly turning a provenance-bearing
        // comparison into a comparison with a constant.
        if opens_with_value_source_kind(text) {
            return Err(E::custom(format!(
                "`{text}` reads as a string literal, but names a value source kind; \
                 a value reference is a map with `source` and `path`"
            )));
        }

        Ok(SelectorValue::Literal(Literal::String(text.to_string())))
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SelectorValue::Literal(Literal::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SelectorValue::Literal(Literal::Int(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        i64::try_from(value)
            .map(|value| SelectorValue::Literal(Literal::Int(value)))
            .map_err(|_| E::custom(format!("`{value}` does not fit an int literal")))
    }
}

/// A `MapAccess` that replays one already-read key before the rest of
/// the map, so a peeked key can still be handed to a derived reader.
struct Replayed<A> {
    key: Option<String>,
    rest: A,
}

impl<A> Replayed<A> {
    fn new(key: String, rest: A) -> Self {
        Replayed {
            key: Some(key),
            rest,
        }
    }
}

impl<'de, A> MapAccess<'de> for Replayed<A>
where
    A: MapAccess<'de>,
{
    type Error = A::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        match self.key.take() {
            Some(key) => seed
                .deserialize(StrDeserializer::<Self::Error>::new(&key))
                .map(Some),
            None => self.rest.next_key_seed(seed),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        self.rest.next_value_seed(seed)
    }

    fn size_hint(&self) -> Option<usize> {
        self.rest
            .size_hint()
            .map(|rest| rest + usize::from(self.key.is_some()))
    }
}

/// A constant value.
///
/// The canonical form is the tagged map; a plain scalar carries the
/// same thing, typed as YAML types it. A string that YAML would read
/// as a bool or an int is written quoted, as it is anywhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Literal {
    String(String),
    Bool(bool),
    Int(i64),
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    rename = "Literal"
)]
enum LiteralLong {
    String(String),
    Bool(bool),
    Int(i64),
}

impl From<LiteralLong> for Literal {
    fn from(long: LiteralLong) -> Self {
        match long {
            LiteralLong::String(value) => Literal::String(value),
            LiteralLong::Bool(value) => Literal::Bool(value),
            LiteralLong::Int(value) => Literal::Int(value),
        }
    }
}

impl<'de> Deserialize<'de> for Literal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LiteralVisitor)
    }
}

struct LiteralVisitor;

impl<'de> Visitor<'de> for LiteralVisitor {
    type Value = Literal;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "a literal: a scalar such as `pending`, `true`, or `3`, \
             or a map with `kind` and `value`",
        )
    }

    fn visit_str<E>(self, text: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Literal::String(text.to_string()))
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Literal::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Literal::Int(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        i64::try_from(value)
            .map(Literal::Int)
            .map_err(|_| E::custom(format!("`{value}` does not fit an int literal")))
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        LiteralLong::deserialize(MapAccessDeserializer::new(map)).map(Literal::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateTransition {
    pub machine: Id,
    pub transition: Id,

    /// Selects the concrete persistent machine instance.
    pub subject: ObjectSelector,

    /// Provenance of each side-effect instance implicitly established
    /// by applying the transition, keyed by side effect.
    ///
    /// The keys must exactly match the transition's declared side
    /// effects; a transition without side effects uses an empty map.
    /// The derivations are evaluated in the enclosing transaction
    /// context at this step, so they may reference preceding
    /// transaction reads.
    pub effect_values: BTreeMap<Id, Derivation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishEffectIntent {
    pub intent: Id,

    /// Provenance of the intent's logical contents.
    pub values: Derivation,
}

/// Exports a typed value from the transaction into the enclosing
/// operation's control.
///
/// For `EstablishTransactionOutput(O, D)` the transaction constructs a
/// value shaped by `O`'s schema, declares its provenance through `D`,
/// establishes `O` atomically with its commit, and makes `O` available
/// to the operation control that follows a successful execution or a
/// commit recovery. It implies no response, no success or failure, no
/// effect execution, no idempotency, and no storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishTransactionOutput {
    pub output: Id,

    /// Provenance of the output's logical contents.
    pub values: Derivation,
}
