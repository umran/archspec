use serde::{Deserialize, Serialize};

use crate::spec::{FieldPath, Id, Model, Schema, TypeRef};

/// The fully expanded form of a field path: every fragment alias along
/// the path is substituted by its source mapping, and the root is the
/// canonical schema reached after expanding outermost fragments.
///
/// A fragment mapping asserts semantic identity of the referenced
/// value across the fragment boundary (§4). Two paths evaluated
/// against instances of the same schema therefore denote the same
/// logical value whenever their canonical forms are equal, even when
/// the declared paths differ through renaming or aliasing.
///
/// Equality of canonical forms is sufficient for value identity, not
/// necessary: unequal forms mean identity is not established, not
/// that the values are proven distinct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalValuePath {
    pub schema: Id,
    pub path: FieldPath,
}

/// Termination guard for fragment expansion.
///
/// Validation rejects fragment cycles, but verification must stay
/// total on models it was not promised; expansion beyond this budget
/// resolves to `None`.
const MAX_FRAGMENT_HOPS: usize = 64;

/// Resolves a field path to its canonical form, or `None` when the
/// path does not resolve against the declared schemas.
pub fn canonical_value_path(
    model: &Model,
    schema: &Id,
    path: &FieldPath,
) -> Option<CanonicalValuePath> {
    if path.0.is_empty() {
        return None;
    }

    let mut hops = 0;

    canonicalize(model, schema, path.0.clone(), &mut hops)
}

fn canonicalize(
    model: &Model,
    schema: &Id,
    mut components: Vec<String>,
    hops: &mut usize,
) -> Option<CanonicalValuePath> {
    let mut schema = schema;

    loop {
        match model.schemas.get(schema)? {
            Schema::Fragment(fragment) => {
                *hops += 1;

                if *hops > MAX_FRAGMENT_HOPS {
                    return None;
                }

                let mapped = fragment.mapping.get(components.first()?)?;

                let mut substituted = mapped.0.clone();

                substituted.extend_from_slice(&components[1..]);

                components = substituted;
                schema = &fragment.source;
            }

            Schema::Canonical(canonical) => {
                let head = components.first()?;

                let field = canonical.fields.get(head)?;

                if components.len() == 1 {
                    return Some(CanonicalValuePath {
                        schema: schema.clone(),
                        path: FieldPath(components),
                    });
                }

                // The nested schema reached from here is a function of
                // this schema and the leading component, so dropping
                // the inner root and nesting the expanded remainder
                // keeps canonical-form equality sound.
                let TypeRef::Schema(inner) = &field.ty else {
                    // V1 defines no traversal through scalars or
                    // collections.
                    return None;
                };

                let rest = canonicalize(model, inner, components[1..].to_vec(), hops)?;

                let mut expanded = vec![head.clone()];

                expanded.extend(rest.path.0);

                return Some(CanonicalValuePath {
                    schema: schema.clone(),
                    path: FieldPath(expanded),
                });
            }
        }
    }
}
