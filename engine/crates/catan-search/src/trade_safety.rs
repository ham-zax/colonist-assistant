use std::collections::HashSet;

use colonist_catan_core::{Action, GameState, Phase, ResourceHand};

/// A domestic trade is hard-vetoed only when the posterior makes one of these
/// concrete, near-term opponent outcomes effectively certain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomesticTradeThreat {
    ImmediateWin,
    AwardSwing,
    ContestedSettlement,
}

/// Keep ordinary uncertain opponent plans in strategic search. This guard is
/// reserved for threats supported by essentially the entire weighted belief.
pub const HARD_VETO_POSTERIOR: f32 = 0.99;

const TACTICAL_BUILD_DEPTH: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ThreatKey {
    ImmediateWin,
    AwardSwing,
    ContestedSettlement(u8),
}

#[derive(Clone, Copy)]
struct Transfer {
    protected: u8,
    beneficiary: u8,
    protected_gives: ResourceHand,
    protected_receives: ResourceHand,
}

fn contains(hand: &ResourceHand, cards: &ResourceHand) -> bool {
    hand.iter()
        .zip(cards)
        .all(|(available, required)| available >= required)
}

fn transfer_candidates(state: &GameState, action: &Action) -> Vec<Transfer> {
    let protected = state.actor();
    match action {
        Action::OfferTrade {
            recipients,
            give,
            receive,
        } if state.phase == Phase::Main && protected == state.current_player => {
            (0..state.board.num_players)
                .filter(|partner| {
                    partner != &protected
                        && recipients & (1 << partner) != 0
                        && contains(&state.players[*partner as usize].resources, receive)
                })
                .map(|beneficiary| Transfer {
                    protected,
                    beneficiary,
                    protected_gives: *give,
                    protected_receives: *receive,
                })
                .collect()
        }
        Action::RespondTrade { accept: true } if state.phase == Phase::TradeResponses => state
            .trade
            .filter(|trade| {
                trade.creator != protected
                    && trade.recipients & (1 << protected) != 0
                    && contains(&state.players[protected as usize].resources, &trade.receive)
            })
            .map(|trade| {
                vec![Transfer {
                    protected,
                    beneficiary: trade.creator,
                    protected_gives: trade.receive,
                    protected_receives: trade.give,
                }]
            })
            .unwrap_or_default(),
        Action::CounterTrade { give, receive }
            if state.phase == Phase::TradeResponses
                && state.trade.is_some_and(|trade| trade.creator != protected) =>
        {
            let beneficiary = state.trade.expect("checked above").creator;
            if contains(&state.players[beneficiary as usize].resources, receive) {
                vec![Transfer {
                    protected,
                    beneficiary,
                    protected_gives: *give,
                    protected_receives: *receive,
                }]
            } else {
                Vec::new()
            }
        }
        Action::ConfirmTrade { partner } if state.phase == Phase::TradeResponses => state
            .trade
            .filter(|trade| {
                trade.creator == protected
                    && *partner != protected
                    && trade.accepted & (1 << partner) != 0
            })
            .map(|trade| {
                vec![Transfer {
                    protected,
                    beneficiary: *partner,
                    protected_gives: trade.give,
                    protected_receives: trade.receive,
                }]
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn main_phase_for(state: &GameState, player: u8) -> GameState {
    let mut result = state.clone();
    result.current_player = player;
    result.phase = Phase::Main;
    result.trade = None;
    result.trade_negotiation_round = 0;
    result
}

fn after_transfer(state: &GameState, transfer: Transfer) -> Option<GameState> {
    if transfer.protected as usize >= state.players.len()
        || transfer.beneficiary as usize >= state.players.len()
        || !contains(
            &state.players[transfer.protected as usize].resources,
            &transfer.protected_gives,
        )
        || !contains(
            &state.players[transfer.beneficiary as usize].resources,
            &transfer.protected_receives,
        )
    {
        return None;
    }
    let mut result = main_phase_for(state, transfer.beneficiary);
    for resource in 0..5 {
        result.players[transfer.protected as usize].resources[resource] =
            result.players[transfer.protected as usize].resources[resource]
                - transfer.protected_gives[resource]
                + transfer.protected_receives[resource];
        result.players[transfer.beneficiary as usize].resources[resource] =
            result.players[transfer.beneficiary as usize].resources[resource]
                - transfer.protected_receives[resource]
                + transfer.protected_gives[resource];
    }
    Some(result)
}

fn is_build(action: &Action) -> bool {
    matches!(
        action,
        Action::BuildRoad { .. } | Action::BuildSettlement { .. } | Action::BuildCity { .. }
    )
}

fn contested_settlement(
    public_state: &GameState,
    vertex: u8,
    protected: u8,
    beneficiary: u8,
) -> bool {
    let Some(candidate) = public_state.board.vertices.get(vertex as usize) else {
        return false;
    };
    if public_state.buildings[vertex as usize].is_some()
        || candidate
            .adjacent_vertices
            .iter()
            .any(|neighbor| public_state.buildings[*neighbor as usize].is_some())
    {
        return false;
    }
    let connected = |player| {
        candidate
            .adjacent_edges
            .iter()
            .any(|edge| public_state.roads[*edge as usize] == Some(player))
    };
    connected(protected) && connected(beneficiary)
}

fn record_threats(
    baseline: &GameState,
    next: &GameState,
    action: &Action,
    protected: u8,
    beneficiary: u8,
    result: &mut HashSet<ThreatKey>,
) {
    if next.winner() == Some(beneficiary) {
        result.insert(ThreatKey::ImmediateWin);
    }
    let beneficiary_gained_award = (baseline.longest_road_holder != Some(beneficiary)
        && next.longest_road_holder == Some(beneficiary))
        || (baseline.largest_army_holder != Some(beneficiary)
            && next.largest_army_holder == Some(beneficiary));
    let protected_lost_award = (baseline.longest_road_holder == Some(protected)
        && next.longest_road_holder != Some(protected))
        || (baseline.largest_army_holder == Some(protected)
            && next.largest_army_holder != Some(protected));
    if beneficiary_gained_award || protected_lost_award {
        result.insert(ThreatKey::AwardSwing);
    }
    if let Action::BuildSettlement { vertex } = action
        && contested_settlement(baseline, *vertex, protected, beneficiary)
    {
        result.insert(ThreatKey::ContestedSettlement(*vertex));
    }
}

fn reachable_build_threats(
    root: &GameState,
    public_baseline: &GameState,
    protected: u8,
    beneficiary: u8,
) -> HashSet<ThreatKey> {
    fn visit(
        state: &GameState,
        public_baseline: &GameState,
        protected: u8,
        beneficiary: u8,
        depth: u8,
        threats: &mut HashSet<ThreatKey>,
        seen: &mut HashSet<(u64, u8)>,
    ) {
        if depth >= TACTICAL_BUILD_DEPTH || !seen.insert((state.state_hash(), depth)) {
            return;
        }
        for action in state.legal_actions().into_iter().filter(is_build) {
            let mut next = state.clone();
            if next.apply(&action).is_err() {
                continue;
            }
            record_threats(
                public_baseline,
                &next,
                &action,
                protected,
                beneficiary,
                threats,
            );
            if !next.is_terminal() {
                visit(
                    &next,
                    public_baseline,
                    protected,
                    beneficiary,
                    depth + 1,
                    threats,
                    seen,
                );
            }
        }
    }

    let mut threats = HashSet::new();
    visit(
        root,
        public_baseline,
        protected,
        beneficiary,
        0,
        &mut threats,
        &mut HashSet::new(),
    );
    threats
}

fn strongest(threats: &HashSet<ThreatKey>) -> Option<DomesticTradeThreat> {
    if threats.contains(&ThreatKey::ImmediateWin) {
        Some(DomesticTradeThreat::ImmediateWin)
    } else if threats.contains(&ThreatKey::AwardSwing) {
        Some(DomesticTradeThreat::AwardSwing)
    } else if threats
        .iter()
        .any(|threat| matches!(threat, ThreatKey::ContestedSettlement(_)))
    {
        Some(DomesticTradeThreat::ContestedSettlement)
    } else {
        None
    }
}

/// Assess one fully specified hidden world. A threat must be newly reachable
/// because of the exchange; an opponent build they could already make does not
/// turn an otherwise useful trade into a hard veto.
pub fn domestic_trade_threat(state: &GameState, action: &Action) -> Option<DomesticTradeThreat> {
    let mut newly_enabled = HashSet::new();
    for transfer in transfer_candidates(state, action) {
        let before = main_phase_for(state, transfer.beneficiary);
        let Some(after) = after_transfer(state, transfer) else {
            continue;
        };
        let before_threats =
            reachable_build_threats(&before, state, transfer.protected, transfer.beneficiary);
        let after_threats =
            reachable_build_threats(&after, state, transfer.protected, transfer.beneficiary);
        newly_enabled.extend(after_threats.difference(&before_threats).copied());
    }
    strongest(&newly_enabled)
}

/// Aggregate the safety proof over a weighted hidden-state belief. Possibility
/// alone is not enough: a hard veto requires at least 99% posterior support.
pub fn belief_domestic_trade_threat<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    action: &Action,
) -> Option<DomesticTradeThreat> {
    if !matches!(
        action,
        Action::OfferTrade { .. }
            | Action::RespondTrade { accept: true }
            | Action::CounterTrade { .. }
            | Action::ConfirmTrade { .. }
    ) {
        return None;
    }
    let worlds = worlds.into_iter().collect::<Vec<_>>();
    let total = worlds
        .iter()
        .map(|(_, weight)| weight.max(0.0))
        .sum::<f32>();
    if total <= f32::EPSILON {
        return None;
    }
    let mut mass = [0.0_f32; 3];
    for (state, weight) in worlds {
        let weight = weight.max(0.0) / total;
        match domestic_trade_threat(state, action) {
            Some(DomesticTradeThreat::ImmediateWin) => mass[0] += weight,
            Some(DomesticTradeThreat::AwardSwing) => mass[1] += weight,
            Some(DomesticTradeThreat::ContestedSettlement) => mass[2] += weight,
            None => {}
        }
    }
    if mass.iter().sum::<f32>() + 1e-6 < HARD_VETO_POSTERIOR {
        None
    } else if mass[0] >= mass[1] && mass[0] >= mass[2] {
        Some(DomesticTradeThreat::ImmediateWin)
    } else if mass[1] >= mass[2] {
        Some(DomesticTradeThreat::AwardSwing)
    } else {
        Some(DomesticTradeThreat::ContestedSettlement)
    }
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, Building, GameState, Phase, TradeOffer};

    use super::{DomesticTradeThreat, belief_domestic_trade_threat, domestic_trade_threat};
    use crate::exact::{ExactActionFamily, solve_exact_belief};
    use crate::mcts::BeliefParticle;

    fn city_threat_state() -> (GameState, Action) {
        let mut state = GameState::standard(401, 3);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.buildings.fill(None);
        state.roads.fill(None);
        state.buildings[0] = Some(Building::Settlement(1));
        state.players[1].public_victory_points = 9;
        state.players[1].resources = [1, 0, 0, 2, 2];
        state.players[0].resources = [0, 0, 0, 0, 1];
        let action = Action::OfferTrade {
            recipients: 1 << 1,
            give: [0, 0, 0, 0, 1],
            receive: [1, 0, 0, 0, 0],
        };
        (state, action)
    }

    #[test]
    fn vetoes_a_trade_that_enables_an_immediate_opponent_win() {
        let (state, action) = city_threat_state();
        assert_eq!(
            domestic_trade_threat(&state, &action),
            Some(DomesticTradeThreat::ImmediateWin)
        );
    }

    #[test]
    fn detects_a_visible_contested_settlement_race() {
        let mut state = GameState::standard(403, 3);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.buildings.fill(None);
        state.roads.fill(None);
        let (vertex, edges) = state
            .board
            .vertices
            .iter()
            .enumerate()
            .find(|(_, vertex)| vertex.adjacent_edges.len() >= 2)
            .map(|(vertex, candidate)| (vertex as u8, candidate.adjacent_edges[..2].to_vec()))
            .expect("standard board has a branching vertex");
        state.roads[edges[0] as usize] = Some(0);
        state.roads[edges[1] as usize] = Some(1);
        state.players[0].resources = [0, 1, 0, 0, 0];
        state.players[1].resources = [1, 0, 1, 1, 1];
        let action = Action::OfferTrade {
            recipients: 1 << 1,
            give: [0, 1, 0, 0, 0],
            receive: [0, 0, 0, 0, 1],
        };

        assert_eq!(
            domestic_trade_threat(&state, &action),
            Some(DomesticTradeThreat::ContestedSettlement),
            "trade must not hand the shared vertex to the opponent at {vertex}"
        );
    }

    #[test]
    fn uncertain_hidden_threat_does_not_become_a_hard_veto() {
        let (dangerous, action) = city_threat_state();
        let mut safe = dangerous.clone();
        safe.players[1].resources = [1, 0, 0, 1, 3];

        assert_eq!(
            belief_domestic_trade_threat([(&dangerous, 0.50), (&safe, 0.50)], &action,),
            None
        );
        assert_eq!(
            belief_domestic_trade_threat([(&dangerous, 0.995), (&safe, 0.005)], &action,),
            Some(DomesticTradeThreat::ImmediateWin)
        );
    }

    #[test]
    fn incoming_acceptance_is_vetoed_when_it_feeds_a_forced_win() {
        let (mut state, _) = city_threat_state();
        state.phase = Phase::TradeResponses;
        state.current_player = 1;
        state.trade_cursor = 0;
        state.trade = Some(TradeOffer {
            creator: 1,
            recipients: 1,
            give: [1, 0, 0, 0, 0],
            receive: [0, 0, 0, 0, 1],
            accepted: 0,
            rejected: 0,
        });

        assert_eq!(
            domestic_trade_threat(&state, &Action::RespondTrade { accept: true },),
            Some(DomesticTradeThreat::ImmediateWin)
        );
        assert_eq!(
            domestic_trade_threat(&state, &Action::RespondTrade { accept: false },),
            None
        );

        let result = solve_exact_belief(
            &[BeliefParticle { state, weight: 1.0 }],
            ExactActionFamily::Mandatory,
        );
        assert_eq!(result.chosen, Some(Action::RespondTrade { accept: false }));
        let accepted = result
            .actions
            .iter()
            .find(|candidate| candidate.action == Action::RespondTrade { accept: true })
            .expect("acceptance remains visible in exact diagnostics");
        assert_eq!(accepted.decision_score, f32::NEG_INFINITY);
    }

    #[test]
    fn harmless_material_exchange_remains_available() {
        let mut state = GameState::standard(409, 3);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [0, 1, 0, 0, 0];
        state.players[1].resources = [0, 0, 1, 0, 0];
        let action = Action::OfferTrade {
            recipients: 1 << 1,
            give: [0, 1, 0, 0, 0],
            receive: [0, 0, 1, 0, 0],
        };
        assert_eq!(domestic_trade_threat(&state, &action), None);
    }
}
