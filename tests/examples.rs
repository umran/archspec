//! The worked example models must stay valid and keep every obligation
//! proven, except where the model deliberately leaves the checker an
//! honest gap — and then exactly that gap, for exactly that reason.

use std::{
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    analyzer::{
        report::{self, Status},
        validation,
        verification::{self, DecisionGap, IdempotencyObstacle, IdempotencyVerdict, ResultGap},
    },
    parser::yaml,
    spec::{Id, Model},
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

    assert!(
        errors.is_empty(),
        "video streaming example should validate:\n{errors:#?}"
    );
}

#[test]
fn video_streaming_example_proves_everything_but_the_external_branch() {
    let model = load("video_streaming.yaml");

    let verification = verification::verify(&model);

    // The transcoder branches on the engine's result. The engine
    // deduplicates renders by video_id, but no declared fact says a
    // repeated render returns the same result, so a retry is not
    // established to take the same arm — and a retried upload, whose
    // cascade reaches the transcoder, inherits the gap.
    let transcode = verification
        .idempotency
        .iter()
        .find(|check| check.operation == Id("operation.transcode_video".into()))
        .expect("transcode_video declares idempotency");

    let IdempotencyVerdict::Unproven { obstacles } = &transcode.verdict else {
        panic!(
            "expected transcode_video unproven, found {:?}",
            transcode.verdict
        );
    };

    assert!(
        matches!(
            &obstacles[..],
            [IdempotencyObstacle::PathDecisionUnstable {
                gap: DecisionGap::ResultUnstable {
                    gap: ResultGap::ExternalResultUndeclared,
                    ..
                },
                ..
            }]
        ),
        "expected only the external-result decision obstacle:\n{obstacles:#?}"
    );

    let upload = verification
        .idempotency
        .iter()
        .find(|check| check.operation == Id("operation.complete_upload".into()))
        .expect("complete_upload declares idempotency");

    assert!(
        matches!(
            &upload.verdict,
            IdempotencyVerdict::Unproven { obstacles }
                if matches!(
                    &obstacles[..],
                    [IdempotencyObstacle::PublicationConsumerRequirementUnproven { operation, .. }]
                        if operation == &Id("operation.transcode_video".into())
                )
        ),
        "expected complete_upload unproven only through its cascade:\n{:#?}",
        upload.verdict
    );

    let report = report::obligations(&model, &verification);

    assert_eq!(report.obligations.len(), 15);

    let unknown: Vec<&str> = report
        .obligations
        .iter()
        .filter(|obligation| obligation.status == Status::Unknown)
        .map(|obligation| obligation.id.as_str())
        .collect();

    assert_eq!(
        unknown,
        [
            "oblig.operation.complete_upload.idempotency.0",
            "oblig.operation.transcode_video.idempotency.0",
        ]
    );

    assert_eq!(
        report
            .obligations
            .iter()
            .filter(|obligation| obligation.status == Status::Proven)
            .count(),
        13
    );
}
