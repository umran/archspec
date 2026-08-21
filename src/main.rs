//! archspec: validates an Archspec model and verifies its declared
//! requirements.
//!
//! Validation errors are fatal: verification is meaningful only over a
//! structurally coherent model. Verification verdicts are epistemic —
//! unproven obligations are reported as notes with the checker's
//! evidence, never as errors — and the full obligation report, with
//! proofs rendered as the facts they rely on, can be written as JSON
//! for tooling such as archspec-viz.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use archspec::analyzer::{self, Diagnostic, Severity, report};

struct Args {
    model: PathBuf,
    report: Option<PathBuf>,
}

const USAGE: &str = "\
archspec — validate and verify an Archspec model

USAGE:
    archspec <MODEL.yaml> [OPTIONS]

OPTIONS:
    --report <PATH>    Write the obligation report (JSON), consumable
                       by archspec-viz --report.
    -h, --help         Show this help.
";

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<ExitCode, String> {
    let source = std::fs::read_to_string(&args.model)
        .map_err(|error| format!("cannot read {}: {error}", args.model.display()))?;

    let model = archspec::parser::yaml::parse(&source)
        .map_err(|error| format!("cannot parse {}: {error}", args.model.display()))?;

    let errors = analyzer::validate(&model);

    if !errors.is_empty() {
        for error in errors {
            print_diagnostic(&Diagnostic::from(error));
        }

        eprintln!("model is invalid; verification not attempted");

        return Ok(ExitCode::FAILURE);
    }

    let verification = analyzer::verification::verify(&model);

    for diagnostic in verification.diagnostics() {
        print_diagnostic(&diagnostic);
    }

    let obligations = report::obligations(&model, &verification);

    let mut proven = 0usize;
    let mut disproven = 0usize;
    let mut unknown = 0usize;

    for obligation in &obligations.obligations {
        match obligation.status {
            report::Status::Proven => proven += 1,
            report::Status::Disproven => disproven += 1,
            report::Status::Unknown => unknown += 1,
        }
    }

    println!(
        "obligations: {proven} proven, {unknown} unknown, {disproven} disproven \
         ({} total)",
        obligations.obligations.len()
    );

    if let Some(path) = &args.report {
        let json = serde_json::to_string_pretty(&obligations)
            .map_err(|error| format!("cannot serialize report: {error}"))?;

        write_output(path, &format!("{json}\n"))?;

        eprintln!("wrote {}", path.display());
    }

    Ok(ExitCode::SUCCESS)
}

fn write_output(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn print_diagnostic(diagnostic: &Diagnostic) {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Unknown => "note",
    };

    let subject = diagnostic
        .subject
        .as_ref()
        .map(|id| format!(" [{id}]"))
        .unwrap_or_default();

    eprintln!("{severity}{subject}: {}", diagnostic.message);

    for evidence in &diagnostic.evidence {
        let subject = evidence
            .subject
            .as_ref()
            .map(|id| format!("[{id}] "))
            .unwrap_or_default();

        eprintln!("    {subject}{}", evidence.message);
    }
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut model = None;
    let mut report = None;

    let mut argv = std::env::args().skip(1);

    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--report" => {
                report = Some(PathBuf::from(
                    argv.next().ok_or("--report requires a value")?,
                ));
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option {other}"));
            }
            _ => {
                if model.replace(PathBuf::from(&arg)).is_some() {
                    return Err("more than one model path given".to_string());
                }
            }
        }
    }

    let Some(model) = model else {
        return Err("no model path given".to_string());
    };

    Ok(Some(Args { model, report }))
}
