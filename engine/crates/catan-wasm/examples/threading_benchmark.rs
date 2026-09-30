//! Replay externally supplied requests without clocks for threading parity,
//! or retain their original budgets with `live`. Output is JSONL diagnostics.

use std::time::Instant;

use serde_json::{Value, json};

fn prepare(request: &mut Value, mode: &str) {
    if mode == "fixed" {
        request["timeBudgetMs"] = 0.into();
        request["depth"] = 2.into();
        request["effort"]["decisionTimeMs"] = 0.into();
        request["effort"]["cpu"]["evidenceEscalationMs"] = 0.into();
        request["effort"]["cpu"]["maxDepth"] = 2.into();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: threading_benchmark requests.json [fixed|live] [reps]")?;
    let mode = args.next().unwrap_or_else(|| "fixed".into());
    if !matches!(mode.as_str(), "fixed" | "live") {
        return Err("mode must be fixed or live".into());
    }
    let reps: usize = args.next().unwrap_or_else(|| "1".into()).parse()?;
    let items: Vec<Value> = serde_json::from_slice(&std::fs::read(path)?)?;
    let first = items.first().ok_or("requests must not be empty")?;
    let mut warmup = first["request"].clone();
    prepare(&mut warmup, "fixed");
    colonist_catan_wasm::analyze_benchmark_request(warmup)?;
    for rep in 0..reps {
        let total_started = Instant::now();
        for item in &items {
            let mut request = item["request"].clone();
            prepare(&mut request, &mode);
            let started = Instant::now();
            let response = colonist_catan_wasm::analyze_benchmark_request(request)?;
            println!(
                "{}",
                json!({
                    "rep": rep, "id": item["id"], "ms": started.elapsed().as_secs_f64() * 1000.0,
                    "sig": {
                        "chosen": response["chosen"], "actions": response["actions"],
                        "nodes": response["nodes"], "depth": response["deepestDecisionDepth"],
                        "authority": response["authority"], "deadlineReached": response["deadlineReached"],
                    },
                    "stages": response["searchStages"],
                })
            );
        }
        println!(
            "{}",
            json!({"rep": rep, "sumMs": total_started.elapsed().as_secs_f64() * 1000.0})
        );
    }
    Ok(())
}
