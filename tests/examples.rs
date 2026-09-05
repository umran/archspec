//! The worked example models must stay valid and keep every obligation
//! proven, except where the model deliberately leaves the checker an
//! honest gap — and then exactly that gap, for exactly that reason.

use std::{
    fs,
    path::{Path, PathBuf},
};

use conseqa::{
    analyzer::{
        report::{self, Status},
        validation,
        verification::{self, IdempotencyVerdict},
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
fn video_streaming_example_proves_everything() {
    let model = load("video_streaming.yaml");

    let verification = verification::verify(&model);

    // The transcoder branches on the engine's result. The engine
    // deduplicates renders by video_id — one logical render per video,
    // whose terminal result the guarantee fixes — and a rejected
    // source is a terminal error, so a retried transcode observes the
    // same terminal result and takes the same arm. That closes what
    // was the model's one gap before error dispositions existed: the
    // transcoder's idempotency, and the upload's cascade through it,
    // now prove.
    for operation in ["operation.transcode_video", "operation.complete_upload"] {
        let check = verification
            .idempotency
            .iter()
            .find(|check| check.operation == Id(operation.into()))
            .expect("the operation declares idempotency");

        assert!(
            matches!(check.verdict, IdempotencyVerdict::Proven { .. }),
            "expected {operation} proven:\n{:#?}",
            check.verdict
        );
    }

    let report = report::obligations(&model, &verification);

    assert_eq!(report.obligations.len(), 15);

    let unproven: Vec<&str> = report
        .obligations
        .iter()
        .filter(|obligation| obligation.status != Status::Proven)
        .map(|obligation| obligation.id.as_str())
        .collect();

    assert_eq!(unproven, [""; 0], "every obligation should prove");
}
