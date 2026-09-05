use std::fmt;

use serde::de::value::MapAccessDeserializer;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::spec::Id;

/// A first-class `Result<Ok, Err>` contract: a tagged sum holding
/// exactly one of an `Ok` payload shaped by `ok` or an `Err` payload
/// shaped by `err.schema`.
///
/// Mutual exclusivity is structural. Archspec models the algebraic
/// outcome, not any language's API around it.
///
/// `Err` is a *logical* returned outcome — a synchronous interaction
/// completed and reported a modeled failure, such as a declined card.
/// It is not an interrupted execution: a crash, a timeout, or a lost
/// connection is an idempotency and recoverability question, not an
/// `Err` payload.
///
/// `Ok` is terminal by definition: it resolves the logical interaction.
/// Whether an `Err` does the same is the error's declared
/// [`ErrorDisposition`], which belongs to this contract, not to the
/// schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultType {
    pub ok: Id,
    pub err: ErrorResultType,
}

impl ResultType {
    /// The schema of one variant's payload.
    pub fn schema(&self, variant: ResultVariant) -> &Id {
        match variant {
            ResultVariant::Ok => &self.ok,
            ResultVariant::Err => &self.err.schema,
        }
    }
}

/// The `Err` half of a result contract: the payload schema and the
/// declared disposition of observing that error.
///
/// The disposition is part of the result contract rather than the
/// schema, so one error schema may be terminal in one contract and
/// retryable in another.
///
/// Canonical form is the map with both fields. The shorthand — a bare
/// schema id — declares `disposition: unspecified`; no shorthand may
/// silently declare `terminal` or `retryable`, because `unspecified`
/// is epistemic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorResultType {
    pub schema: Id,
    pub disposition: ErrorDisposition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename = "ErrorResultType")]
struct ErrorResultTypeLong {
    schema: Id,

    #[serde(default)]
    disposition: ErrorDisposition,
}

impl<'de> Deserialize<'de> for ErrorResultType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ErrorResultTypeVisitor)
    }
}

struct ErrorResultTypeVisitor;

impl<'de> Visitor<'de> for ErrorResultTypeVisitor {
    type Value = ErrorResultType;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "an error result contract: a schema id such as `schema.CardDeclined`, \
             or a map with `schema` and `disposition`",
        )
    }

    fn visit_str<E>(self, text: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let schema = Id(text.trim().to_string());

        if schema.0.is_empty() {
            return Err(E::custom("expected a schema id"));
        }

        Ok(ErrorResultType {
            schema,
            disposition: ErrorDisposition::Unspecified,
        })
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let long = ErrorResultTypeLong::deserialize(MapAccessDeserializer::new(map))?;

        Ok(ErrorResultType {
            schema: long.schema,
            disposition: long.disposition,
        })
    }
}

/// Whether observing the contract's `Err` terminally resolves the
/// logical interaction, or conclusively ends one attempt while
/// semantically admitting another.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorDisposition {
    /// No usable fact: the model does not say whether observing this
    /// `Err` terminally resolves the logical interaction or admits
    /// another attempt. Nothing may be inferred.
    #[default]
    Unspecified,

    /// Observing this `Err` terminally resolves the logical
    /// interaction with the declared error payload.
    Terminal,

    /// Observing this `Err` conclusively ends the current attempt but
    /// does not terminally resolve the logical interaction; another
    /// attempt is semantically admitted. It does not say a retry
    /// occurs, succeeds, or returns the same error — those are
    /// execution semantics Archspec does not model here.
    Retryable,
}

impl fmt::Display for ErrorDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unspecified => "unspecified",
            Self::Terminal => "terminal",
            Self::Retryable => "retryable",
        })
    }
}

/// Which arm of a `Result` an outcome, a match arm, or a value source
/// refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultVariant {
    Ok,
    Err,
}

impl fmt::Display for ResultVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Err => "err",
        })
    }
}
