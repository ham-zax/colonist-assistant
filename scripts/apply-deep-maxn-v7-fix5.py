from pathlib import Path

path = Path("engine/crates/catan-arena/src/main.rs")
source = path.read_text()

old = '''    decision_count: [u32; 4],
    decision_time: [Duration; 4],
    trade_value_sum: [f32; 4],
'''
new = '''    decision_count: [u32; 4],
    decision_time: [Duration; 4],
    search_decision_count: [u32; 4],
    search_nodes: [u64; 4],
    search_depth: [u64; 4],
    posterior_particles: [u64; 4],
    strategic_particles: [u64; 4],
    search_deadlines: [u32; 4],
    search_action_values: [u64; 4],
    trade_value_sum: [f32; 4],
'''
if source.count(old) != 1:
    raise SystemExit("GameMetrics insertion point not found")
source = source.replace(old, new)

old = '''    decisions: u64,
    decision_nanos: u128,
    trade_value_sum: f64,
'''
new = '''    decisions: u64,
    decision_nanos: u128,
    search_decisions: u64,
    search_nodes: u64,
    search_depth: u64,
    posterior_particles: u64,
    strategic_particles: u64,
    search_deadlines: u64,
    search_action_values: u64,
    trade_value_sum: f64,
'''
if source.count(old) != 1:
    raise SystemExit("CandidateMetrics insertion point not found")
source = source.replace(old, new)

old = '''            metrics.decision_time[actor] += started.elapsed();
            metrics.decision_count[actor] += 1;
'''
new = '''            metrics.decision_time[actor] += started.elapsed();
            metrics.decision_count[actor] += 1;
            if choice.root_value.is_some() {
                metrics.search_decision_count[actor] += 1;
                metrics.search_nodes[actor] += u64::from(choice.nodes);
                metrics.search_depth[actor] += u64::from(choice.depth);
                metrics.posterior_particles[actor] += choice.posterior_particles as u64;
                metrics.strategic_particles[actor] += choice.strategic_particles as u64;
                metrics.search_deadlines[actor] += u32::from(choice.deadline_reached);
                metrics.search_action_values[actor] += choice.action_values.len() as u64;
            }
'''
if source.count(old) != 1:
    raise SystemExit("choice diagnostic recording point not found")
source = source.replace(old, new)

old = '''            if state.phase == Phase::PreRoll {
                if let Some(root_value) = choice.root_value {
                    calibration.push((actor as u8, root_value[actor]));
                }
            }
'''
new = '''            if state.phase == Phase::PreRoll
                && let Some(root_value) = choice.root_value
            {
                calibration.push((actor as u8, root_value[actor]));
            }
'''
if source.count(old) != 1:
    raise SystemExit("calibration block not found")
source = source.replace(old, new)

old = '''            candidate_metrics.decisions += metrics.decision_count[player] as u64;
            candidate_metrics.decision_nanos += metrics.decision_time[player].as_nanos();
            candidate_metrics.trade_value_sum += metrics.trade_value_sum[player] as f64;
'''
new = '''            candidate_metrics.decisions += metrics.decision_count[player] as u64;
            candidate_metrics.decision_nanos += metrics.decision_time[player].as_nanos();
            candidate_metrics.search_decisions += metrics.search_decision_count[player] as u64;
            candidate_metrics.search_nodes += metrics.search_nodes[player];
            candidate_metrics.search_depth += metrics.search_depth[player];
            candidate_metrics.posterior_particles += metrics.posterior_particles[player];
            candidate_metrics.strategic_particles += metrics.strategic_particles[player];
            candidate_metrics.search_deadlines += metrics.search_deadlines[player] as u64;
            candidate_metrics.search_action_values += metrics.search_action_values[player];
            candidate_metrics.trade_value_sum += metrics.trade_value_sum[player] as f64;
'''
if source.count(old) != 1:
    raise SystemExit("candidate diagnostic aggregation point not found")
source = source.replace(old, new)

old = '''        let seats = metric.seats.max(1) as f64;
        format!(
            "\\\"{}\\\":{{\\\"seatSamples\\\":{},\\\"meanRank\\\":{:.6},\\\"meanVictoryPoints\\\":{:.6}}}",
            engine.as_str(),
            metric.seats,
            metric.ranks / seats,
            metric.points as f64 / seats,
        )
'''
new = '''        let seats = metric.seats.max(1) as f64;
        let searches = metric.search_decisions.max(1) as f64;
        format!(
            "\\\"{}\\\":{{\\\"seatSamples\\\":{},\\\"meanRank\\\":{:.6},\\\"meanVictoryPoints\\\":{:.6},\\\"searchSamples\\\":{},\\\"meanSearchNodes\\\":{:.3},\\\"meanSearchDepth\\\":{:.3},\\\"meanPosteriorParticles\\\":{:.3},\\\"meanStrategicParticles\\\":{:.3},\\\"searchDeadlineShare\\\":{:.6},\\\"meanRootActions\\\":{:.3}}}",
            engine.as_str(),
            metric.seats,
            metric.ranks / seats,
            metric.points as f64 / seats,
            metric.search_decisions,
            metric.search_nodes as f64 / searches,
            metric.search_depth as f64 / searches,
            metric.posterior_particles as f64 / searches,
            metric.strategic_particles as f64 / searches,
            metric.search_deadlines as f64 / searches,
            metric.search_action_values as f64 / searches,
        )
'''
if source.count(old) != 1:
    raise SystemExit("compact engine metrics block not found")
source = source.replace(old, new)

path.write_text(source)
