use std::any::Any;
use std::env;
use std::panic::{AssertUnwindSafe, catch_unwind};

use colonist_catan_arena::{EXACT_PARITY_CORPUS_REVISION, ExactParityCase, exact_parity_corpus};
use colonist_catan_search::{CudaExactEvaluator, evaluate};
use serde::Serialize;

const SCHEMA_VERSION: u8 = 1;
const MAX_FAILURES: usize = 16;
const TOLERANCE: f32 = 1e-5;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceReport {
    backend: &'static str,
    ordinal: usize,
    name: String,
    compute_capability: (i32, i32),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsReport {
    batches: u64,
    states: u64,
    total_ms: f64,
    last_batch_ms: f64,
    last_pack_ms: f64,
    last_upload_ms: f64,
    last_kernel_ms: f64,
    last_download_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    case_index: usize,
    case: String,
    players: u8,
    phase: String,
    reason: String,
    component: Option<usize>,
    cpu: Option<f32>,
    cuda: Option<f32>,
    abs_error: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u8,
    kind: &'static str,
    corpus_revision: &'static str,
    mode: &'static str,
    evaluator: &'static str,
    neural_evaluator: bool,
    cpu_oracle: &'static str,
    player_order: &'static str,
    tolerance: f32,
    parity: bool,
    validated: bool,
    cases: usize,
    three_player_cases: usize,
    four_player_cases: usize,
    terminal_cases: usize,
    nonterminal_cases: usize,
    max_abs_error: f32,
    failure_count: usize,
    validation_failures: usize,
    nonfinite_failures: usize,
    normalization_failures: usize,
    terminal_failures: usize,
    value_failures: usize,
    backend_error: Option<String>,
    device: Option<DeviceReport>,
    stats: Option<StatsReport>,
    failures: Vec<Failure>,
}

impl Report {
    fn new(mode: &'static str, cases: &[ExactParityCase]) -> Self {
        let three_player_cases = cases
            .iter()
            .filter(|case| case.state.board.num_players == 3)
            .count();
        let four_player_cases = cases
            .iter()
            .filter(|case| case.state.board.num_players == 4)
            .count();
        let terminal_cases = cases.iter().filter(|case| case.state.is_terminal()).count();
        Self {
            schema_version: SCHEMA_VERSION,
            kind: "colonist-exact-gpu-parity",
            corpus_revision: EXACT_PARITY_CORPUS_REVISION,
            mode,
            evaluator: "handcrafted-exact",
            neural_evaluator: false,
            cpu_oracle: "colonist_catan_search::evaluate",
            player_order: "canonical-player-id-0..3",
            tolerance: TOLERANCE,
            parity: false,
            validated: false,
            cases: cases.len(),
            three_player_cases,
            four_player_cases,
            terminal_cases,
            nonterminal_cases: cases.len() - terminal_cases,
            max_abs_error: 0.0,
            failure_count: 0,
            validation_failures: 0,
            nonfinite_failures: 0,
            normalization_failures: 0,
            terminal_failures: 0,
            value_failures: 0,
            backend_error: None,
            device: None,
            stats: None,
            failures: Vec::new(),
        }
    }

    fn add_failure(&mut self, failure: Failure) {
        eprintln!(
            "exact-gpu-parity: {} case={} component={:?} cpu={:?} cuda={:?} error={:?}",
            failure.reason,
            failure.case,
            failure.component,
            failure.cpu,
            failure.cuda,
            failure.abs_error,
        );
        self.failure_count += 1;
        if self.failures.len() < MAX_FAILURES {
            self.failures.push(failure);
        }
    }
}

fn parse_batch_size() -> Result<usize, String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        print_help();
        std::process::exit(0);
    }
    let mut batch_size = 256;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--batch-size" => {
                index += 1;
                batch_size = arguments
                    .get(index)
                    .ok_or("--batch-size requires a value")?
                    .parse()
                    .map_err(|_| "--batch-size must be a positive integer")?;
                if batch_size == 0 {
                    return Err("--batch-size must be positive".into());
                }
            }
            argument => return Err(format!("unknown argument: {argument}")),
        }
        index += 1;
    }
    Ok(batch_size)
}

fn print_help() {
    println!(
        "Usage: exact-gpu-parity [--batch-size N]\n\n\
         Compares the fixed Phase-1 handcrafted CPU evaluator against CUDA on\n\
         the deterministic 3P/4P corpus."
    );
}

fn failure(
    case_index: usize,
    case: &ExactParityCase,
    reason: impl Into<String>,
    component: Option<usize>,
    cpu: Option<f32>,
    cuda: Option<f32>,
) -> Failure {
    let abs_error = match (cpu, cuda) {
        (Some(left), Some(right)) if left.is_finite() && right.is_finite() => {
            Some((left - right).abs())
        }
        _ => None,
    };
    Failure {
        case_index,
        case: case.name.clone(),
        players: case.state.board.num_players,
        phase: format!("{:?}", case.state.phase),
        reason: reason.into(),
        component,
        cpu,
        cuda,
        abs_error,
    }
}

fn finite(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "CUDA evaluator panicked with a non-string payload".into()
}

fn validate_cases(report: &mut Report, cases: &[ExactParityCase]) {
    for (index, case) in cases.iter().enumerate() {
        if let Err(error) = case.state.validate() {
            report.validation_failures += 1;
            report.add_failure(failure(
                index,
                case,
                format!("invalid generated state: {error}"),
                None,
                None,
                None,
            ));
            continue;
        }
        let actions = case.state.legal_actions();
        if case.state.is_terminal() {
            if case.state.winner().is_none() {
                report.validation_failures += 1;
                report.add_failure(failure(
                    index,
                    case,
                    "terminal state has no winner",
                    None,
                    None,
                    None,
                ));
            }
            if !actions.is_empty() {
                report.validation_failures += 1;
                report.add_failure(failure(
                    index,
                    case,
                    "terminal state exposes legal actions",
                    None,
                    None,
                    None,
                ));
            }
        } else if actions.is_empty() {
            report.validation_failures += 1;
            report.add_failure(failure(
                index,
                case,
                "nonterminal state has no legal actions",
                None,
                None,
                None,
            ));
        }
    }
    report.validated = report.failure_count == 0;
}

fn check_vector(
    report: &mut Report,
    index: usize,
    case: &ExactParityCase,
    side: &str,
    values: [f32; 4],
) {
    for (component, value) in values.into_iter().enumerate() {
        if !value.is_finite() {
            report.nonfinite_failures += 1;
            report.add_failure(failure(
                index,
                case,
                format!("{side} evaluator returned non-finite output"),
                Some(component),
                (side == "cpu").then_some(value).and_then(finite),
                (side == "cuda").then_some(value).and_then(finite),
            ));
        }
    }
    let player_count = case.state.board.num_players as usize;
    let sum = values
        .iter()
        .take(player_count)
        .copied()
        .filter(|value| value.is_finite())
        .sum::<f32>();
    if values[..player_count].iter().all(|value| value.is_finite()) && (sum - 1.0).abs() > TOLERANCE
    {
        report.normalization_failures += 1;
        report.add_failure(failure(
            index,
            case,
            format!("{side} values are not normalized"),
            None,
            (side == "cpu").then_some(sum),
            (side == "cuda").then_some(sum),
        ));
    }
    for (component, value) in values.iter().copied().enumerate().skip(player_count) {
        if value.is_finite() && value.abs() > TOLERANCE {
            report.value_failures += 1;
            report.add_failure(failure(
                index,
                case,
                format!("{side} emitted a value for an inactive player"),
                Some(component),
                (side == "cpu").then_some(value),
                (side == "cuda").then_some(value),
            ));
        }
    }
}

fn compare_values(
    report: &mut Report,
    index: usize,
    case: &ExactParityCase,
    cpu: [f32; 4],
    cuda: [f32; 4],
) {
    check_vector(report, index, case, "cpu", cpu);
    check_vector(report, index, case, "cuda", cuda);

    if let Some(winner) = case.state.winner() {
        for component in 0..4 {
            let expected = f32::from(u8::from(component == winner as usize));
            if cpu[component] != expected || cuda[component] != expected {
                report.terminal_failures += 1;
                report.add_failure(failure(
                    index,
                    case,
                    "terminal output is not exact one-hot in canonical player order",
                    Some(component),
                    finite(cpu[component]),
                    finite(cuda[component]),
                ));
            }
        }
        return;
    }

    for component in 0..4 {
        if cpu[component].is_finite() && cuda[component].is_finite() {
            let error = (cpu[component] - cuda[component]).abs();
            report.max_abs_error = report.max_abs_error.max(error);
            if error > TOLERANCE {
                report.value_failures += 1;
                report.add_failure(failure(
                    index,
                    case,
                    "nonterminal CPU/CUDA component exceeds tolerance",
                    Some(component),
                    finite(cpu[component]),
                    finite(cuda[component]),
                ));
            }
        }
    }
}

fn stats_report(evaluator: &CudaExactEvaluator) -> StatsReport {
    let stats = evaluator.stats();
    StatsReport {
        batches: stats.batches,
        states: stats.states,
        total_ms: stats.total_nanos as f64 / 1_000_000.0,
        last_batch_ms: stats.last_batch_nanos as f64 / 1_000_000.0,
        last_pack_ms: stats.last_pack_nanos as f64 / 1_000_000.0,
        last_upload_ms: stats.last_upload_nanos as f64 / 1_000_000.0,
        last_kernel_ms: stats.last_kernel_nanos as f64 / 1_000_000.0,
        last_download_ms: stats.last_download_nanos as f64 / 1_000_000.0,
    }
}

fn run_cases(
    mode: &'static str,
    cases: Vec<ExactParityCase>,
    batch_size: usize,
) -> Report {
    let mut report = Report::new(mode, &cases);
    validate_cases(&mut report, &cases);
    if report.failure_count > 0 {
        return report;
    }

    let cpu_values = cases
        .iter()
        .map(|case| evaluate(&case.state))
        .collect::<Vec<_>>();
    let states = cases
        .iter()
        .map(|case| case.state.clone())
        .collect::<Vec<_>>();
    let mut evaluator = match catch_unwind(AssertUnwindSafe(CudaExactEvaluator::new)) {
        Ok(Ok(evaluator)) => evaluator,
        Ok(Err(error)) => {
            report.backend_error = Some(error.to_string());
            eprintln!("exact-gpu-parity: CUDA evaluator initialization failed: {error}");
            return report;
        }
        Err(payload) => {
            let error = panic_message(payload);
            report.backend_error = Some(error.clone());
            eprintln!("exact-gpu-parity: CUDA evaluator initialization panicked: {error}");
            return report;
        }
    };
    let identity = evaluator.device_identity();
    report.device = Some(DeviceReport {
        backend: identity.backend,
        ordinal: identity.ordinal,
        name: identity.name.clone(),
        compute_capability: identity.compute_capability,
    });

    let mut cuda_values = Vec::with_capacity(states.len());
    for chunk in states.chunks(batch_size) {
        match catch_unwind(AssertUnwindSafe(|| evaluator.evaluate_batch(chunk))) {
            Ok(Ok(values)) if values.len() == chunk.len() => cuda_values.extend(values),
            Ok(Ok(values)) => {
                report.backend_error = Some(format!(
                    "CUDA evaluator returned {} values for a batch of {} states",
                    values.len(),
                    chunk.len()
                ));
                eprintln!(
                    "exact-gpu-parity: {}",
                    report.backend_error.as_deref().unwrap()
                );
                return report;
            }
            Ok(Err(error)) => {
                report.backend_error = Some(error.to_string());
                eprintln!("exact-gpu-parity: CUDA batch evaluation failed: {error}");
                return report;
            }
            Err(payload) => {
                let error = panic_message(payload);
                report.backend_error = Some(error.clone());
                eprintln!("exact-gpu-parity: CUDA batch evaluation panicked: {error}");
                return report;
            }
        }
    }
    report.stats = Some(stats_report(&evaluator));

    for (index, ((case, cpu), cuda)) in cases.iter().zip(cpu_values).zip(cuda_values).enumerate() {
        compare_values(&mut report, index, case, cpu, cuda);
    }
    report.parity = report.failure_count == 0 && report.backend_error.is_none();
    report
}

fn main() {
    let batch_size = match parse_batch_size() {
        Ok(batch_size) => batch_size,
        Err(error) => {
            eprintln!("exact-gpu-parity: {error}");
            eprintln!("use --help for usage");
            std::process::exit(2);
        }
    };
    let cases = exact_parity_corpus();
    let report = run_cases("corpus", cases, batch_size);
    println!(
        "{}",
        serde_json::to_string(&report).expect("parity report must serialize")
    );
    if !report.parity {
        std::process::exit(1);
    }
}
