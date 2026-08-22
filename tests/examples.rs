//! The worked example models must stay valid and keep every obligation
//! the checker can attempt proven; only the families V1 does not
//! verify may remain unknown.

use std::{
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    analyzer::{
        report::{self, Property, Status},
        validation, verification,
    },
    parser::yaml,
    spec::Model,
};

fn load(name: &str) -> Model {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);

    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()));

    yaml::parse(&source).unwrap_or_else(|error| panic!("`{name}` should parse: {error}"))
}

#[test]
fn video_streaming_example_is_valid() {
    let model = load("video_streaming.yaml");

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "video streaming example should validate:\n{errors:#?}");
}

#[test]
fn video_streaming_example_proves_every_verifiable_obligation() {
    let model = load("video_streaming.yaml");

    let verification = verification::verify(&model);

    assert!(
        verification.all_proven(),
        "every verified requirement should be proven:\n{:#?}",
        verification.diagnostics()
    );

    let report = report::obligations(&model, &verification);

    assert_eq!(report.obligations.len(), 17);

    for obligation in &report.obligations {
        match obligation.property {
            // Ordering and object history have no V1 verifier.
            Property::Ordering | Property::ObjectHistory => {
                assert_eq!(obligation.status, Status::Unknown, "{}", obligation.id);
            }

            _ => {
                assert_eq!(obligation.status, Status::Proven, "{}", obligation.id);
            }
        }
    }

    assert_eq!(
        report
            .obligations
            .iter()
            .filter(|obligation| obligation.status == Status::Proven)
            .count(),
        13
    );
}
