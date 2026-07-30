use std::fmt;
use std::sync::Arc;

use crate::types::{CITY_COST, DEVELOPMENT_COST, ROAD_COST, SETTLEMENT_COST};
use crate::{
    Action, Board, Building, DevCard, NodeKind, Phase, PlayerState, Port, Resource, ResourceHand,
    SplitMix64, TradeOffer,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleError {
    GameFinished,
    WrongPhase,
    WrongActor,
    InvalidRoll,
    IllegalVertex,
    IllegalEdge,
    Occupied,
    DistanceRule,
    NotConnected,
    InsufficientResources,
    BankShortage,
    PieceUnavailable,
    DevelopmentUnavailable,
    DevelopmentAlreadyPlayed,
    DevelopmentBoughtThisTurn,
    InvalidDiscard,
    InvalidRobber,
    InvalidVictim,
    InvalidTrade,
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RuleError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameState {
    pub board: Arc<Board>,
    pub players: Vec<PlayerState>,
    pub buildings: Vec<Option<Building>>,
    pub roads: Vec<Option<u8>>,
    pub bank: ResourceHand,
    /// Whether resource-specific bank counts are observable. Physical/table
    /// games and Colonist layouts that expose the bank set this true. When
    /// false, information-set hashes redact the bank composition while each
    /// particle still preserves exact conservation internally.
    pub bank_is_public: bool,
    pub development_deck: [u8; 5],
    pub played_development: [u8; 5],
    pub robber_hex: u8,
    pub current_player: u8,
    pub phase: Phase,
    pub turn: u16,
    pub last_roll: u8,
    pub victory_target: u8,
    pub setup_step: u8,
    pub discard_remaining: [u8; 4],
    pub discard_cursor: u8,
    pub robber_return_phase: Phase,
    pub free_roads: u8,
    pub domestic_trade_used: bool,
    pub domestic_trade_count: u8,
    pub last_rejected_trade: Option<TradeOffer>,
    pub trade: Option<TradeOffer>,
    pub trade_cursor: u8,
    pub trade_negotiation_round: u8,
    pub longest_road_holder: Option<u8>,
    pub largest_army_holder: Option<u8>,
}

impl GameState {
    pub fn new(board: Board, victory_target: u8) -> Self {
        let num_players = board.num_players;
        let robber_hex = board
            .hexes
            .iter()
            .position(|hex| hex.resource.is_none())
            .unwrap_or(0) as u8;
        Self {
            players: vec![PlayerState::new(); num_players as usize],
            buildings: vec![None; board.vertices.len()],
            roads: vec![None; board.edges.len()],
            board: Arc::new(board),
            bank: [19; 5],
            bank_is_public: true,
            development_deck: [14, 5, 2, 2, 2],
            played_development: [0; 5],
            robber_hex,
            current_player: 0,
            phase: Phase::SetupSettlement,
            turn: 0,
            last_roll: 0,
            victory_target,
            setup_step: 0,
            discard_remaining: [0; 4],
            discard_cursor: 0,
            robber_return_phase: Phase::Main,
            free_roads: 0,
            domestic_trade_used: false,
            domestic_trade_count: 0,
            last_rejected_trade: None,
            trade: None,
            trade_cursor: 0,
            trade_negotiation_round: 0,
            longest_road_holder: None,
            largest_army_holder: None,
        }
    }

    pub fn standard(seed: u64, num_players: u8) -> Self {
        Self::new(Board::standard(seed, num_players), 10)
    }

    pub fn actor(&self) -> u8 {
        match self.phase {
            Phase::Discard => self.discard_cursor,
            Phase::TradeResponses => {
                let Some(trade) = self.trade else {
                    return self.current_player;
                };
                if self.trade_responses_complete(trade) {
                    trade.creator
                } else {
                    self.trade_cursor
                }
            }
            _ => self.current_player,
        }
    }

    pub fn winner(&self) -> Option<u8> {
        if self.phase != Phase::Finished {
            return None;
        }
        self.players
            .iter()
            .enumerate()
            .find(|(_, player)| player.victory_points() >= self.victory_target)
            .map(|(index, _)| index as u8)
    }

    pub fn is_terminal(&self) -> bool {
        self.phase == Phase::Finished
    }

    pub fn node_kind(&self) -> NodeKind {
        match self.phase {
            Phase::Finished => NodeKind::Terminal,
            Phase::RollChance | Phase::DevelopmentChance | Phase::ResolveSteal { .. } => {
                NodeKind::Chance
            }
            _ => NodeKind::Decision {
                actor: self.actor(),
            },
        }
    }

    pub fn chance_weight(&self, action: &Action) -> u16 {
        match (self.phase, action) {
            (Phase::RollChance, Action::ResolveRoll { value }) => {
                const DICE_WEIGHTS: [u16; 13] = [0, 0, 1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1];
                DICE_WEIGHTS.get(*value as usize).copied().unwrap_or(0)
            }
            (Phase::DevelopmentChance, Action::ResolveDevelopment { card }) => {
                self.development_deck[card.index()] as u16
            }
            (
                Phase::ResolveSteal { victim },
                Action::ResolveSteal {
                    victim: action_victim,
                    resource,
                },
            ) if victim == *action_victim => {
                self.players[victim as usize].resources[resource.index()] as u16
            }
            _ => 0,
        }
    }

    pub fn sample_chance(&self, rng: &mut SplitMix64) -> Option<Action> {
        if self.node_kind() != NodeKind::Chance {
            return None;
        }
        let actions = self.legal_actions();
        let total = actions
            .iter()
            .map(|action| self.chance_weight(action) as usize)
            .sum::<usize>();
        if total == 0 {
            return None;
        }
        let mut target = rng.range(total);
        for action in actions {
            let weight = self.chance_weight(&action) as usize;
            if target < weight {
                return Some(action);
            }
            target -= weight;
        }
        None
    }

    /// Checks conservation, piece inventory, topology, and score bookkeeping.
    ///
    /// The arena calls this after every transition in validation builds. It is
    /// intentionally independent from `apply` so production search does not
    /// pay for a full scan of the board on every simulated action.
    pub fn validate(&self) -> Result<(), String> {
        let players = self.board.num_players as usize;
        if !(2..=4).contains(&players) || self.players.len() != players {
            return Err("player count does not match board".into());
        }
        if self.buildings.len() != self.board.vertices.len()
            || self.roads.len() != self.board.edges.len()
        {
            return Err("dynamic topology length does not match board".into());
        }
        for resource in Resource::ALL {
            let total = self.bank[resource.index()] as u16
                + self
                    .players
                    .iter()
                    .map(|player| player.resources[resource.index()] as u16)
                    .sum::<u16>();
            if total != 19 {
                return Err(format!(
                    "{resource:?} conservation failed: expected 19, found {total}"
                ));
            }
        }
        const DEVELOPMENT_TOTALS: [u16; 5] = [14, 5, 2, 2, 2];
        for card in DevCard::ALL {
            let index = card.index();
            let total = self.development_deck[index] as u16
                + self.played_development[index] as u16
                + self
                    .players
                    .iter()
                    .map(|player| player.development[index] as u16)
                    .sum::<u16>();
            if total != DEVELOPMENT_TOTALS[index] {
                return Err(format!(
                    "{card:?} conservation failed: expected {}, found {total}",
                    DEVELOPMENT_TOTALS[index]
                ));
            }
            if self
                .players
                .iter()
                .any(|player| player.bought_development[index] > player.development[index])
            {
                return Err(format!("{card:?} bought-this-turn count exceeds hand"));
            }
        }
        for player in 0..players {
            let player_id = player as u8;
            let roads = self
                .roads
                .iter()
                .filter(|owner| **owner == Some(player_id))
                .count();
            let settlements = self
                .buildings
                .iter()
                .filter(|building| **building == Some(Building::Settlement(player_id)))
                .count();
            let cities = self
                .buildings
                .iter()
                .filter(|building| **building == Some(Building::City(player_id)))
                .count();
            let state = &self.players[player];
            if roads + state.roads_left as usize != 15
                || settlements + state.settlements_left as usize != 5
                || cities + state.cities_left as usize != 4
            {
                return Err(format!("piece inventory failed for player {player}"));
            }
            let expected_public = settlements as u8
                + (cities as u8 * 2)
                + u8::from(state.has_longest_road) * 2
                + u8::from(state.has_largest_army) * 2;
            if state.public_victory_points != expected_public {
                return Err(format!(
                    "public score failed for player {player}: expected {expected_public}, found {}",
                    state.public_victory_points
                ));
            }
        }
        for (vertex, building) in self.buildings.iter().enumerate() {
            if building.is_none() {
                continue;
            }
            if self.board.vertices[vertex]
                .adjacent_vertices
                .iter()
                .any(|neighbor| self.buildings[*neighbor as usize].is_some())
            {
                return Err(format!("distance rule failed at vertex {vertex}"));
            }
        }
        if self
            .roads
            .iter()
            .flatten()
            .any(|owner| *owner >= self.board.num_players)
            || self
                .buildings
                .iter()
                .flatten()
                .any(|building| building.player() >= self.board.num_players)
        {
            return Err("piece owner is outside player range".into());
        }
        if let Some(holder) = self.longest_road_holder
            && (!self.players[holder as usize].has_longest_road
                || self.longest_road_length(holder) < 5)
        {
            return Err("Longest Road holder is inconsistent".into());
        }
        if let Some(holder) = self.largest_army_holder
            && (!self.players[holder as usize].has_largest_army
                || self.players[holder as usize].played_knights < 3)
        {
            return Err("Largest Army holder is inconsistent".into());
        }
        Ok(())
    }

    /// Stable FNV-1a hash used by replay traces and cross-build parity tests.
    pub fn state_hash(&self) -> u64 {
        fn byte(hash: &mut u64, value: u8) {
            *hash ^= value as u64;
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        fn hand(hash: &mut u64, values: &[u8]) {
            for value in values {
                byte(hash, *value);
            }
        }
        fn phase(hash: &mut u64, value: Phase) {
            match value {
                Phase::SetupSettlement => byte(hash, 0),
                Phase::SetupRoad { settlement } => {
                    byte(hash, 1);
                    byte(hash, settlement);
                }
                Phase::PreRoll => byte(hash, 2),
                Phase::RollChance => byte(hash, 3),
                Phase::Discard => byte(hash, 4),
                Phase::MoveRobber => byte(hash, 5),
                Phase::ResolveSteal { victim } => {
                    byte(hash, 6);
                    byte(hash, victim);
                }
                Phase::Main => byte(hash, 7),
                Phase::DevelopmentChance => byte(hash, 8),
                Phase::TradeResponses => byte(hash, 9),
                Phase::Finished => byte(hash, 10),
            }
        }
        let mut hash = 0xcbf2_9ce4_8422_2325;
        byte(&mut hash, self.board.num_players);
        for hex in &self.board.hexes {
            byte(
                &mut hash,
                hex.resource.map(|resource| resource as u8 + 1).unwrap_or(0),
            );
            byte(&mut hash, hex.number);
        }
        for vertex in &self.board.vertices {
            let port = match vertex.port {
                None => 0,
                Some(Port::Generic) => 1,
                Some(Port::Resource(resource)) => resource as u8 + 2,
            };
            byte(&mut hash, port);
        }
        for player in &self.players {
            hand(&mut hash, &player.resources);
            hand(&mut hash, &player.development);
            hand(&mut hash, &player.bought_development);
            hand(&mut hash, &player.policy_profile);
            hand(
                &mut hash,
                &[
                    player.public_victory_points,
                    player.played_knights,
                    player.roads_left,
                    player.settlements_left,
                    player.cities_left,
                    u8::from(player.has_longest_road),
                    u8::from(player.has_largest_army),
                    u8::from(player.played_development_this_turn),
                ],
            );
        }
        for building in &self.buildings {
            byte(
                &mut hash,
                match building {
                    None => 0,
                    Some(Building::Settlement(player)) => 1 + *player,
                    Some(Building::City(player)) => 5 + *player,
                },
            );
        }
        for road in &self.roads {
            byte(&mut hash, road.map(|player| player + 1).unwrap_or(0));
        }
        hand(&mut hash, &self.bank);
        hand(&mut hash, &self.development_deck);
        hand(&mut hash, &self.played_development);
        hand(
            &mut hash,
            &[
                self.robber_hex,
                self.current_player,
                self.last_roll,
                self.victory_target,
                self.setup_step,
                self.discard_cursor,
                self.free_roads,
                u8::from(self.domestic_trade_used),
                u8::from(self.bank_is_public),
                self.domestic_trade_count,
                self.trade_cursor,
                self.trade_negotiation_round,
            ],
        );
        for byte_value in self.turn.to_le_bytes() {
            byte(&mut hash, byte_value);
        }
        hand(&mut hash, &self.discard_remaining);
        phase(&mut hash, self.phase);
        phase(&mut hash, self.robber_return_phase);
        if let Some(trade) = self.trade {
            byte(&mut hash, 1);
            hand(
                &mut hash,
                &[
                    trade.creator,
                    trade.recipients,
                    trade.accepted,
                    trade.rejected,
                ],
            );
            hand(&mut hash, &trade.give);
            hand(&mut hash, &trade.receive);
        } else {
            byte(&mut hash, 0);
        }
        if let Some(trade) = self.last_rejected_trade {
            byte(&mut hash, 1);
            hand(&mut hash, &trade.give);
            hand(&mut hash, &trade.receive);
        } else {
            byte(&mut hash, 0);
        }
        byte(
            &mut hash,
            self.longest_road_holder
                .map(|player| player + 1)
                .unwrap_or(0),
        );
        byte(
            &mut hash,
            self.largest_army_holder
                .map(|player| player + 1)
                .unwrap_or(0),
        );
        hash
    }

    /// Canonical state containing exactly the information available to one
    /// observer.
    ///
    /// The acting player's own resource and development identities remain
    /// exact. Every other private hand, the hidden development deck, and a
    /// non-public bank are reduced to their observable totals. Policies and
    /// learned priors must consume this view rather than a determinized state;
    /// the simulator still applies the selected action to the exact particle.
    pub fn observed_state(&self, observer: u8) -> Self {
        let mut observation = self.clone();
        for (player, state) in observation.players.iter_mut().enumerate() {
            if player == observer as usize {
                continue;
            }
            let resource_total = state.resource_total();
            let development_total = state.development.iter().sum::<u8>();
            let bought_total = state.bought_development.iter().sum::<u8>();
            state.resources = [resource_total, 0, 0, 0, 0];
            state.development = [development_total, 0, 0, 0, 0];
            state.bought_development = [bought_total, 0, 0, 0, 0];
        }
        let deck_total = observation.development_deck.iter().sum::<u8>();
        observation.development_deck = [deck_total, 0, 0, 0, 0];
        if !observation.bank_is_public {
            let bank_total = observation.bank.iter().sum::<u8>();
            observation.bank = [bank_total, 0, 0, 0, 0];
        }
        observation
    }

    /// Hash of everything an observer is allowed to condition a policy on.
    ///
    /// Other players' resource and development identities are replaced by
    /// their public totals. The observer's own hand remains exact. This is used
    /// by information-set search tests to catch accidental hidden-state reads.
    pub fn observation_hash(&self, observer: u8) -> u64 {
        self.observed_state(observer).state_hash()
    }

    /// Public-state hash with every private hand redacted to its public total.
    pub fn public_hash(&self) -> u64 {
        let mut observation = self.clone();
        for state in &mut observation.players {
            let resource_total = state.resource_total();
            let development_total = state.development.iter().sum::<u8>();
            let bought_total = state.bought_development.iter().sum::<u8>();
            state.resources = [resource_total, 0, 0, 0, 0];
            state.development = [development_total, 0, 0, 0, 0];
            state.bought_development = [bought_total, 0, 0, 0, 0];
        }
        let deck_total = observation.development_deck.iter().sum::<u8>();
        observation.development_deck = [deck_total, 0, 0, 0, 0];
        if !observation.bank_is_public {
            let bank_total = observation.bank.iter().sum::<u8>();
            observation.bank = [bank_total, 0, 0, 0, 0];
        }
        observation.state_hash()
    }

    pub fn legal_actions(&self) -> Vec<Action> {
        if self.phase == Phase::Finished {
            return Vec::new();
        }
        match self.phase {
            Phase::SetupSettlement => self
                .board
                .vertices
                .iter()
                .enumerate()
                .filter(|(vertex, _)| self.can_place_settlement(*vertex as u8, true))
                .map(|(vertex, _)| Action::PlaceSettlement {
                    vertex: vertex as u8,
                })
                .collect(),
            Phase::SetupRoad { settlement } => self.board.vertices[settlement as usize]
                .adjacent_edges
                .iter()
                .copied()
                .filter(|edge| self.roads[*edge as usize].is_none())
                .map(|edge| Action::PlaceRoad { edge })
                .collect(),
            Phase::PreRoll => {
                let mut actions = vec![Action::Roll];
                actions.extend(self.playable_development_actions(true));
                actions
            }
            Phase::RollChance => (2..=12)
                .map(|value| Action::ResolveRoll { value })
                .collect(),
            Phase::Discard => self.discard_actions(self.discard_cursor),
            Phase::MoveRobber => self.robber_actions(),
            Phase::ResolveSteal { victim } => Resource::ALL
                .iter()
                .copied()
                .filter(|resource| self.players[victim as usize].resources[resource.index()] > 0)
                .map(|resource| Action::ResolveSteal { victim, resource })
                .collect(),
            Phase::Main => self.main_actions(),
            Phase::DevelopmentChance => DevCard::ALL
                .iter()
                .copied()
                .filter(|card| self.development_deck[card.index()] > 0)
                .map(|card| Action::ResolveDevelopment { card })
                .collect(),
            Phase::TradeResponses => self.trade_actions(),
            Phase::Finished => Vec::new(),
        }
    }

    pub fn apply(&mut self, action: &Action) -> Result<(), RuleError> {
        let mut next = self.clone();
        next.apply_in_place(action)?;
        *self = next;
        Ok(())
    }

    fn apply_in_place(&mut self, action: &Action) -> Result<(), RuleError> {
        if self.phase == Phase::Finished {
            return Err(RuleError::GameFinished);
        }
        match action {
            Action::PlaceSettlement { vertex } => self.place_setup_settlement(*vertex),
            Action::PlaceRoad { edge } => self.place_setup_road(*edge),
            Action::Roll => self.start_roll(),
            Action::ResolveRoll { value } => self.resolve_roll(*value),
            Action::Discard { cards } => self.discard(*cards),
            Action::MoveRobber { hex, victim } => self.move_robber(*hex, *victim),
            Action::ResolveSteal { victim, resource } => self.resolve_steal(*victim, *resource),
            Action::BuildRoad { edge } => self.build_road(*edge, false),
            Action::BuildSettlement { vertex } => self.build_settlement(*vertex),
            Action::BuildCity { vertex } => self.build_city(*vertex),
            Action::BuyDevelopment => self.buy_development(),
            Action::ResolveDevelopment { card } => self.resolve_development(*card),
            Action::PlayKnight { hex, victim } => self.play_knight(*hex, *victim),
            Action::PlayRoadBuilding { first, second } => self.play_road_building(*first, *second),
            Action::PlayYearOfPlenty { first, second } => self.play_year_of_plenty(*first, *second),
            Action::PlayMonopoly { resource } => self.play_monopoly(*resource),
            Action::MaritimeTrade {
                give,
                receive,
                ratio,
            } => self.maritime_trade(*give, *receive, *ratio),
            Action::OfferTrade {
                recipients,
                give,
                receive,
            } => self.offer_trade(*recipients, *give, *receive),
            Action::RespondTrade { accept } => self.respond_trade(*accept),
            Action::CounterTrade { give, receive } => self.counter_trade(*give, *receive),
            Action::ConfirmTrade { partner } => self.confirm_trade(*partner),
            Action::CancelTrade => self.cancel_trade(),
            Action::EndTurn => self.end_turn(),
        }?;
        self.finish_if_won();
        Ok(())
    }

    fn finish_if_won(&mut self) {
        if self.players[self.current_player as usize].victory_points() >= self.victory_target {
            self.phase = Phase::Finished;
        }
    }

    fn can_place_settlement(&self, vertex: u8, setup: bool) -> bool {
        let Some(candidate) = self.board.vertices.get(vertex as usize) else {
            return false;
        };
        if self.buildings[vertex as usize].is_some() {
            return false;
        }
        if candidate
            .adjacent_vertices
            .iter()
            .any(|neighbor| self.buildings[*neighbor as usize].is_some())
        {
            return false;
        }
        setup
            || candidate
                .adjacent_edges
                .iter()
                .any(|edge| self.roads[*edge as usize] == Some(self.current_player))
    }

    fn place_setup_settlement(&mut self, vertex: u8) -> Result<(), RuleError> {
        if self.phase != Phase::SetupSettlement {
            return Err(RuleError::WrongPhase);
        }
        if !self.can_place_settlement(vertex, true) {
            return Err(RuleError::DistanceRule);
        }
        self.place_settlement_piece(vertex)?;
        if self.setup_step >= self.board.num_players {
            let adjacent = self.board.vertices[vertex as usize].adjacent_hexes.clone();
            for hex in adjacent {
                if let Some(resource) = self.board.hexes[hex as usize].resource
                    && self.bank[resource.index()] > 0
                {
                    self.bank[resource.index()] -= 1;
                    self.players[self.current_player as usize].resources[resource.index()] += 1;
                }
            }
        }
        self.phase = Phase::SetupRoad { settlement: vertex };
        Ok(())
    }

    fn place_setup_road(&mut self, edge: u8) -> Result<(), RuleError> {
        let Phase::SetupRoad { settlement } = self.phase else {
            return Err(RuleError::WrongPhase);
        };
        let Some(candidate) = self.board.edges.get(edge as usize) else {
            return Err(RuleError::IllegalEdge);
        };
        if self.roads[edge as usize].is_some() {
            return Err(RuleError::Occupied);
        }
        if !candidate.vertices.contains(&settlement) {
            return Err(RuleError::NotConnected);
        }
        self.place_road_piece(edge)?;
        self.setup_step += 1;
        let total = self.board.num_players * 2;
        if self.setup_step >= total {
            self.current_player = 0;
            self.phase = Phase::PreRoll;
            self.turn = 1;
        } else {
            self.current_player = if self.setup_step < self.board.num_players {
                self.setup_step
            } else {
                total - self.setup_step - 1
            };
            self.phase = Phase::SetupSettlement;
        }
        Ok(())
    }

    fn start_roll(&mut self) -> Result<(), RuleError> {
        if self.phase != Phase::PreRoll {
            return Err(RuleError::WrongPhase);
        }
        self.phase = Phase::RollChance;
        Ok(())
    }

    fn resolve_roll(&mut self, value: u8) -> Result<(), RuleError> {
        if self.phase != Phase::RollChance {
            return Err(RuleError::WrongPhase);
        }
        if !(2..=12).contains(&value) {
            return Err(RuleError::InvalidRoll);
        }
        self.last_roll = value;
        if value == 7 {
            self.discard_remaining = [0; 4];
            for player in 0..self.board.num_players {
                let total = self.players[player as usize].resource_total();
                if total > 7 {
                    self.discard_remaining[player as usize] = total / 2;
                }
            }
            if let Some(player) = self.next_discarder(0) {
                self.discard_cursor = player;
                self.phase = Phase::Discard;
            } else {
                self.robber_return_phase = Phase::Main;
                self.phase = Phase::MoveRobber;
            }
        } else {
            self.produce(value);
            self.phase = Phase::Main;
        }
        Ok(())
    }

    fn produce(&mut self, roll: u8) {
        let mut demand = vec![[0u8; 5]; self.board.num_players as usize];
        for (vertex, building) in self.buildings.iter().enumerate() {
            let Some(building) = building else {
                continue;
            };
            let player = building.player() as usize;
            for hex in &self.board.vertices[vertex].adjacent_hexes {
                let tile = &self.board.hexes[*hex as usize];
                if *hex == self.robber_hex || tile.number != roll {
                    continue;
                }
                if let Some(resource) = tile.resource {
                    demand[player][resource.index()] += building.production_multiplier();
                }
            }
        }
        for resource in Resource::ALL {
            let total = demand.iter().map(|hand| hand[resource.index()]).sum::<u8>();
            if total > self.bank[resource.index()] {
                continue;
            }
            self.bank[resource.index()] -= total;
            for (player, hand) in demand.iter().enumerate() {
                self.players[player].resources[resource.index()] += hand[resource.index()];
            }
        }
    }

    fn discard_actions(&self, player: u8) -> Vec<Action> {
        let required = self.discard_remaining[player as usize];
        let hand = self.players[player as usize].resources;
        let mut result = Vec::new();
        let mut current = [0; 5];
        fn visit(
            index: usize,
            remaining: u8,
            hand: ResourceHand,
            current: &mut ResourceHand,
            output: &mut Vec<Action>,
        ) {
            if index == 5 {
                if remaining == 0 {
                    output.push(Action::Discard { cards: *current });
                }
                return;
            }
            for count in 0..=remaining.min(hand[index]) {
                current[index] = count;
                visit(index + 1, remaining - count, hand, current, output);
            }
            current[index] = 0;
        }
        visit(0, required, hand, &mut current, &mut result);
        result
    }

    fn discard(&mut self, cards: ResourceHand) -> Result<(), RuleError> {
        if self.phase != Phase::Discard {
            return Err(RuleError::WrongPhase);
        }
        let player = self.discard_cursor as usize;
        if cards.iter().copied().sum::<u8>() != self.discard_remaining[player]
            || !contains(&self.players[player].resources, &cards)
        {
            return Err(RuleError::InvalidDiscard);
        }
        subtract(&mut self.players[player].resources, &cards);
        add(&mut self.bank, &cards);
        self.discard_remaining[player] = 0;
        if let Some(next) = self.next_discarder(self.discard_cursor + 1) {
            self.discard_cursor = next;
        } else {
            self.robber_return_phase = Phase::Main;
            self.phase = Phase::MoveRobber;
        }
        Ok(())
    }

    fn next_discarder(&self, start: u8) -> Option<u8> {
        (start..self.board.num_players).find(|player| self.discard_remaining[*player as usize] > 0)
    }

    fn robber_actions(&self) -> Vec<Action> {
        let mut result = Vec::new();
        for hex in 0..self.board.hexes.len() as u8 {
            if hex == self.robber_hex {
                continue;
            }
            let mut victims = 0u8;
            for (vertex, building) in self.buildings.iter().enumerate() {
                let Some(building) = building else {
                    continue;
                };
                let player = building.player();
                if player == self.current_player
                    || self.players[player as usize].resource_total() == 0
                    || !self.board.vertices[vertex].adjacent_hexes.contains(&hex)
                {
                    continue;
                }
                victims |= 1 << player;
            }
            if victims == 0 {
                result.push(Action::MoveRobber { hex, victim: None });
            } else {
                for player in 0..self.board.num_players {
                    if victims & (1 << player) != 0 {
                        result.push(Action::MoveRobber {
                            hex,
                            victim: Some(player),
                        });
                    }
                }
            }
        }
        result
    }

    fn move_robber(&mut self, hex: u8, victim: Option<u8>) -> Result<(), RuleError> {
        if self.phase != Phase::MoveRobber {
            return Err(RuleError::WrongPhase);
        }
        if hex as usize >= self.board.hexes.len() || hex == self.robber_hex {
            return Err(RuleError::InvalidRobber);
        }
        if !self
            .robber_actions()
            .contains(&Action::MoveRobber { hex, victim })
        {
            return Err(RuleError::InvalidVictim);
        }
        self.robber_hex = hex;
        self.phase = match victim {
            Some(victim) => Phase::ResolveSteal { victim },
            None => self.robber_return_phase,
        };
        Ok(())
    }

    fn resolve_steal(&mut self, victim: u8, resource: Resource) -> Result<(), RuleError> {
        if self.phase != (Phase::ResolveSteal { victim }) {
            return Err(RuleError::WrongPhase);
        }
        if self.players[victim as usize].resources[resource.index()] == 0 {
            return Err(RuleError::InvalidVictim);
        }
        self.players[victim as usize].resources[resource.index()] -= 1;
        self.players[self.current_player as usize].resources[resource.index()] += 1;
        self.phase = self.robber_return_phase;
        Ok(())
    }

    fn main_actions(&self) -> Vec<Action> {
        let mut actions = vec![Action::EndTurn];
        let player = &self.players[self.current_player as usize];
        if player.roads_left > 0 && contains(&player.resources, &ROAD_COST) {
            actions.extend(
                (0..self.board.edges.len() as u8)
                    .filter(|edge| self.can_build_road(*edge))
                    .map(|edge| Action::BuildRoad { edge }),
            );
        }
        if player.settlements_left > 0 && contains(&player.resources, &SETTLEMENT_COST) {
            actions.extend(
                (0..self.board.vertices.len() as u8)
                    .filter(|vertex| self.can_place_settlement(*vertex, false))
                    .map(|vertex| Action::BuildSettlement { vertex }),
            );
        }
        if player.cities_left > 0 && contains(&player.resources, &CITY_COST) {
            actions.extend(
                self.buildings
                    .iter()
                    .enumerate()
                    .filter_map(|(vertex, building)| match building {
                        Some(Building::Settlement(owner)) if *owner == self.current_player => {
                            Some(Action::BuildCity {
                                vertex: vertex as u8,
                            })
                        }
                        _ => None,
                    }),
            );
        }
        if contains(&player.resources, &DEVELOPMENT_COST)
            && self.development_deck.iter().any(|count| *count > 0)
        {
            actions.push(Action::BuyDevelopment);
        }
        actions.extend(self.playable_development_actions(false));
        for give in Resource::ALL {
            let ratio = self.trade_ratios(self.current_player)[give.index()];
            if player.resources[give.index()] >= ratio {
                for receive in Resource::ALL {
                    if give != receive && self.bank[receive.index()] > 0 {
                        actions.push(Action::MaritimeTrade {
                            give,
                            receive,
                            ratio,
                        });
                    }
                }
            }
        }
        if self.domestic_trade_count < 2 {
            actions.extend(self.generated_domestic_trade_offers());
        }
        actions
    }

    fn generated_domestic_trade_offers(&self) -> Vec<Action> {
        let player = &self.players[self.current_player as usize];
        let recipients = ((1u8 << self.board.num_players) - 1) & !(1u8 << self.current_player);
        // Generate deficits from complete conversion plans as well as atomic
        // builds. This makes an offer able to unlock road → settlement and
        // two-road → settlement endpoints instead of asking only for the next
        // isolated click.
        let costs = [
            ROAD_COST,
            SETTLEMENT_COST,
            CITY_COST,
            DEVELOPMENT_COST,
            [2, 2, 1, 1, 0],
            [3, 3, 1, 1, 0],
        ];
        let mut requests = Vec::<ResourceHand>::new();
        for cost in costs {
            let mut missing = [0; 5];
            for index in 0..5 {
                missing[index] = cost[index].saturating_sub(player.resources[index]);
                if missing[index] > 0 {
                    let mut request = [0; 5];
                    request[index] = 1;
                    requests.push(request);
                    if missing[index] >= 2 {
                        request[index] = 2;
                        requests.push(request);
                    }
                }
            }
            for first in 0..5 {
                if missing[first] == 0 {
                    continue;
                }
                for second in first + 1..5 {
                    if missing[second] == 0 {
                        continue;
                    }
                    let mut request = [0; 5];
                    request[first] = 1;
                    request[second] = 1;
                    requests.push(request);
                }
            }
        }
        requests.sort_unstable();
        requests.dedup();
        if player.resource_total() > 7 {
            // A high-risk hand still needs a useful request even when an
            // atomic cost is already affordable. One-card requests let the
            // planner compare a hand-safety conversion against ending above 7.
            for index in 0..5 {
                let mut request = [0; 5];
                request[index] = 1;
                requests.push(request);
            }
            requests.sort_unstable();
            requests.dedup();
        }

        let mut offers = Vec::<ResourceHand>::new();
        for first in 0..5 {
            if player.resources[first] == 0 {
                continue;
            }
            let mut offer = [0; 5];
            offer[first] = 1;
            offers.push(offer);
            if player.resources[first] >= 2 {
                offer[first] = 2;
                offers.push(offer);
            }
            for second in first + 1..5 {
                if player.resources[second] == 0 {
                    continue;
                }
                let mut mixed = [0; 5];
                mixed[first] = 1;
                mixed[second] = 1;
                offers.push(mixed);
            }
        }

        let ratios = self.trade_ratios(self.current_player);
        let mut actions = Vec::new();
        for receive in requests {
            for give in &offers {
                if Resource::ALL
                    .iter()
                    .any(|resource| give[resource.index()] > 0 && receive[resource.index()] > 0)
                {
                    continue;
                }
                if self
                    .last_rejected_trade
                    .is_some_and(|trade| trade.give == *give && trade.receive == receive)
                {
                    continue;
                }
                let give_total = give.iter().copied().sum::<u8>();
                let receive_total = receive.iter().copied().sum::<u8>();
                let maritime_dominated = receive_total == 1
                    && Resource::ALL
                        .iter()
                        .any(|resource| give[resource.index()] >= ratios[resource.index()]);
                if maritime_dominated || give_total == 0 || give_total > 2 || receive_total > 2 {
                    continue;
                }
                actions.push(Action::OfferTrade {
                    recipients,
                    give: *give,
                    receive,
                });
            }
        }
        // The bounded list must not depend on resource-array lexicographic
        // order. Rank offers by the complete build plans they unlock, hand
        // safety, and bundle efficiency before applying the cap. Search adds
        // opponent acceptance and race value; this rules-layer ordering only
        // guarantees that useful mixed bundles survive candidate generation.
        actions.sort_by(|left, right| {
            let score = |action: &Action| {
                let Action::OfferTrade { give, receive, .. } = action else {
                    return f32::NEG_INFINITY;
                };
                let mut after = player.resources;
                for resource in 0..5 {
                    after[resource] = after[resource]
                        .saturating_sub(give[resource])
                        .saturating_add(receive[resource]);
                }
                let completed = [
                    (ROAD_COST, 1.2_f32),
                    (SETTLEMENT_COST, 7.5),
                    (CITY_COST, 7.0),
                    (DEVELOPMENT_COST, 3.4),
                    ([2, 2, 1, 1, 0], 8.8),
                    ([3, 3, 1, 1, 0], 9.4),
                ]
                .iter()
                .filter(|(cost, _)| contains(&after, cost))
                .map(|(_, value)| *value)
                .fold(0.0_f32, f32::max);
                let nearest = [ROAD_COST, SETTLEMENT_COST, CITY_COST, DEVELOPMENT_COST]
                    .iter()
                    .map(|cost| {
                        cost.iter()
                            .enumerate()
                            .map(|(resource, required)| {
                                required.saturating_sub(after[resource]) as f32
                            })
                            .sum::<f32>()
                    })
                    .fold(f32::INFINITY, f32::min);
                let give_total = give.iter().copied().sum::<u8>() as f32;
                let receive_total = receive.iter().copied().sum::<u8>() as f32;
                let safety = if player.resource_total() > 7 {
                    (give_total - receive_total).max(0.0) * 0.8
                } else {
                    0.0
                };
                completed + 1.5 / (1.0 + nearest) + receive_total * 0.32 - give_total * 0.18
                    + safety
            };
            score(right)
                .total_cmp(&score(left))
                .then_with(|| format!("{left:?}").cmp(&format!("{right:?}")))
        });
        actions.truncate(96);
        actions
    }

    fn playable_development_actions(&self, pre_roll: bool) -> Vec<Action> {
        let player = &self.players[self.current_player as usize];
        if player.played_development_this_turn {
            return Vec::new();
        }
        let playable = |card: DevCard| {
            player.development[card.index()] > player.bought_development[card.index()]
        };
        let mut actions = Vec::new();
        if playable(DevCard::Knight) {
            for robber in self.robber_actions_for_dev() {
                let Action::MoveRobber { hex, victim } = robber else {
                    continue;
                };
                actions.push(Action::PlayKnight { hex, victim });
            }
        }
        if pre_roll {
            return actions;
        }
        if playable(DevCard::RoadBuilding) && player.roads_left > 0 {
            let first_roads = (0..self.board.edges.len() as u8)
                .filter(|edge| self.can_build_road(*edge))
                .collect::<Vec<_>>();
            for first in first_roads {
                let mut after_first = self.clone();
                after_first
                    .build_road(first, true)
                    .expect("generated first free road must be legal");
                let second_roads = (0..after_first.board.edges.len() as u8)
                    .filter(|edge| after_first.can_build_road(*edge))
                    .collect::<Vec<_>>();
                if second_roads.is_empty() || player.roads_left == 1 {
                    actions.push(Action::PlayRoadBuilding {
                        first,
                        second: None,
                    });
                } else {
                    actions.extend(second_roads.into_iter().map(|second| {
                        Action::PlayRoadBuilding {
                            first,
                            second: Some(second),
                        }
                    }));
                }
            }
        }
        if playable(DevCard::YearOfPlenty) {
            for (first_index, first) in Resource::ALL.iter().copied().enumerate() {
                for second in Resource::ALL.iter().copied().skip(first_index) {
                    let needed = if first == second { 2 } else { 1 };
                    if self.bank[first.index()] >= needed && self.bank[second.index()] > 0 {
                        actions.push(Action::PlayYearOfPlenty { first, second });
                    }
                }
            }
        }
        if playable(DevCard::Monopoly) {
            actions.extend(
                Resource::ALL
                    .iter()
                    .copied()
                    .map(|resource| Action::PlayMonopoly { resource }),
            );
        }
        actions
    }

    fn robber_actions_for_dev(&self) -> Vec<Action> {
        let mut clone = self.clone();
        clone.phase = Phase::MoveRobber;
        clone.robber_actions()
    }

    fn can_build_road(&self, edge: u8) -> bool {
        let Some(candidate) = self.board.edges.get(edge as usize) else {
            return false;
        };
        if self.roads[edge as usize].is_some() {
            return false;
        }
        candidate.vertices.iter().any(|vertex| {
            if let Some(building) = self.buildings[*vertex as usize] {
                return building.player() == self.current_player;
            }
            self.board.vertices[*vertex as usize]
                .adjacent_edges
                .iter()
                .any(|neighbor| {
                    *neighbor != edge && self.roads[*neighbor as usize] == Some(self.current_player)
                })
        })
    }

    fn build_road(&mut self, edge: u8, free: bool) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        if !self.can_build_road(edge) {
            return Err(RuleError::NotConnected);
        }
        if !free {
            self.pay_current(&ROAD_COST)?;
        }
        self.place_road_piece(edge)?;
        self.update_longest_road();
        Ok(())
    }

    fn build_settlement(&mut self, vertex: u8) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        if !self.can_place_settlement(vertex, false) {
            return Err(RuleError::NotConnected);
        }
        self.pay_current(&SETTLEMENT_COST)?;
        self.place_settlement_piece(vertex)?;
        self.update_longest_road();
        Ok(())
    }

    fn place_settlement_piece(&mut self, vertex: u8) -> Result<(), RuleError> {
        let player = &mut self.players[self.current_player as usize];
        if player.settlements_left == 0 {
            return Err(RuleError::PieceUnavailable);
        }
        if self.buildings.get(vertex as usize).is_none() {
            return Err(RuleError::IllegalVertex);
        }
        player.settlements_left -= 1;
        player.public_victory_points += 1;
        self.buildings[vertex as usize] = Some(Building::Settlement(self.current_player));
        Ok(())
    }

    fn place_road_piece(&mut self, edge: u8) -> Result<(), RuleError> {
        let player = &mut self.players[self.current_player as usize];
        if player.roads_left == 0 {
            return Err(RuleError::PieceUnavailable);
        }
        if self.roads.get(edge as usize).is_none() {
            return Err(RuleError::IllegalEdge);
        }
        player.roads_left -= 1;
        self.roads[edge as usize] = Some(self.current_player);
        Ok(())
    }

    fn build_city(&mut self, vertex: u8) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        if self.buildings.get(vertex as usize)
            != Some(&Some(Building::Settlement(self.current_player)))
        {
            return Err(RuleError::IllegalVertex);
        }
        if self.players[self.current_player as usize].cities_left == 0 {
            return Err(RuleError::PieceUnavailable);
        }
        self.pay_current(&CITY_COST)?;
        let player = &mut self.players[self.current_player as usize];
        player.cities_left -= 1;
        player.settlements_left += 1;
        player.public_victory_points += 1;
        self.buildings[vertex as usize] = Some(Building::City(self.current_player));
        Ok(())
    }

    fn buy_development(&mut self) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        if self.development_deck.iter().all(|count| *count == 0) {
            return Err(RuleError::DevelopmentUnavailable);
        }
        self.pay_current(&DEVELOPMENT_COST)?;
        self.phase = Phase::DevelopmentChance;
        Ok(())
    }

    fn resolve_development(&mut self, card: DevCard) -> Result<(), RuleError> {
        if self.phase != Phase::DevelopmentChance {
            return Err(RuleError::WrongPhase);
        }
        if self.development_deck[card.index()] == 0 {
            return Err(RuleError::DevelopmentUnavailable);
        }
        self.development_deck[card.index()] -= 1;
        let player = &mut self.players[self.current_player as usize];
        player.development[card.index()] += 1;
        player.bought_development[card.index()] += 1;
        self.phase = Phase::Main;
        Ok(())
    }

    fn consume_development(&mut self, card: DevCard) -> Result<(), RuleError> {
        let player = &mut self.players[self.current_player as usize];
        if player.played_development_this_turn {
            return Err(RuleError::DevelopmentAlreadyPlayed);
        }
        if player.development[card.index()] == 0 {
            return Err(RuleError::DevelopmentUnavailable);
        }
        if player.development[card.index()] <= player.bought_development[card.index()] {
            return Err(RuleError::DevelopmentBoughtThisTurn);
        }
        player.development[card.index()] -= 1;
        self.played_development[card.index()] += 1;
        player.played_development_this_turn = true;
        Ok(())
    }

    fn play_knight(&mut self, hex: u8, victim: Option<u8>) -> Result<(), RuleError> {
        if !matches!(self.phase, Phase::PreRoll | Phase::Main) {
            return Err(RuleError::WrongPhase);
        }
        let return_phase = self.phase;
        self.consume_development(DevCard::Knight)?;
        self.players[self.current_player as usize].played_knights += 1;
        self.update_largest_army();
        self.robber_return_phase = return_phase;
        self.phase = Phase::MoveRobber;
        self.move_robber(hex, victim)
    }

    fn play_road_building(&mut self, first: u8, second: Option<u8>) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        self.consume_development(DevCard::RoadBuilding)?;
        self.build_road(first, true)?;
        if let Some(second) = second {
            self.build_road(second, true)?;
        }
        Ok(())
    }

    fn play_year_of_plenty(&mut self, first: Resource, second: Resource) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        let first_needed = if first == second { 2 } else { 1 };
        if self.bank[first.index()] < first_needed || self.bank[second.index()] == 0 {
            return Err(RuleError::BankShortage);
        }
        self.consume_development(DevCard::YearOfPlenty)?;
        self.bank[first.index()] -= 1;
        self.bank[second.index()] -= 1;
        self.players[self.current_player as usize].resources[first.index()] += 1;
        self.players[self.current_player as usize].resources[second.index()] += 1;
        Ok(())
    }

    fn play_monopoly(&mut self, resource: Resource) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        self.consume_development(DevCard::Monopoly)?;
        let mut total = 0;
        for player in 0..self.board.num_players {
            if player == self.current_player {
                continue;
            }
            total += self.players[player as usize].resources[resource.index()];
            self.players[player as usize].resources[resource.index()] = 0;
        }
        self.players[self.current_player as usize].resources[resource.index()] += total;
        Ok(())
    }

    fn maritime_trade(
        &mut self,
        give: Resource,
        receive: Resource,
        ratio: u8,
    ) -> Result<(), RuleError> {
        if self.phase != Phase::Main || give == receive {
            return Err(RuleError::WrongPhase);
        }
        let actual = self.trade_ratios(self.current_player)[give.index()];
        if ratio != actual
            || self.players[self.current_player as usize].resources[give.index()] < ratio
            || self.bank[receive.index()] == 0
        {
            return Err(RuleError::InvalidTrade);
        }
        self.players[self.current_player as usize].resources[give.index()] -= ratio;
        self.bank[give.index()] += ratio;
        self.bank[receive.index()] -= 1;
        self.players[self.current_player as usize].resources[receive.index()] += 1;
        Ok(())
    }

    pub fn trade_ratios(&self, player: u8) -> ResourceHand {
        let mut ratios = [4; 5];
        for (vertex, building) in self.buildings.iter().enumerate() {
            if building.map(Building::player) != Some(player) {
                continue;
            }
            match self.board.vertices[vertex].port {
                Some(Port::Generic) => {
                    for ratio in &mut ratios {
                        *ratio = (*ratio).min(3);
                    }
                }
                Some(Port::Resource(resource)) => {
                    ratios[resource.index()] = 2;
                }
                None => {}
            }
        }
        ratios
    }

    fn offer_trade(
        &mut self,
        recipients: u8,
        give: ResourceHand,
        receive: ResourceHand,
    ) -> Result<(), RuleError> {
        if self.phase != Phase::Main
            || recipients == 0
            || recipients & (1 << self.current_player) != 0
            || recipients >> self.board.num_players != 0
            || give.iter().copied().sum::<u8>() == 0
            || receive.iter().copied().sum::<u8>() == 0
            || !contains(&self.players[self.current_player as usize].resources, &give)
            || Resource::ALL
                .iter()
                .any(|resource| give[resource.index()] > 0 && receive[resource.index()] > 0)
        {
            return Err(RuleError::InvalidTrade);
        }
        self.trade = Some(TradeOffer {
            creator: self.current_player,
            recipients,
            give,
            receive,
            accepted: 0,
            rejected: 0,
        });
        self.domestic_trade_used = true;
        self.domestic_trade_count = self.domestic_trade_count.saturating_add(1);
        self.trade_cursor = self
            .next_trade_recipient(recipients, 0)
            .ok_or(RuleError::InvalidTrade)?;
        self.phase = Phase::TradeResponses;
        self.trade_negotiation_round = 0;
        Ok(())
    }

    fn trade_actions(&self) -> Vec<Action> {
        let Some(trade) = self.trade else {
            return Vec::new();
        };
        if self.trade_responses_complete(trade) {
            let mut actions = vec![Action::CancelTrade];
            for partner in 0..self.board.num_players {
                if trade.accepted & (1 << partner) != 0
                    && contains(&self.players[trade.creator as usize].resources, &trade.give)
                    && contains(&self.players[partner as usize].resources, &trade.receive)
                {
                    actions.push(Action::ConfirmTrade { partner });
                }
            }
            return actions;
        }
        let mut actions = vec![Action::RespondTrade { accept: false }];
        if contains(
            &self.players[self.trade_cursor as usize].resources,
            &trade.receive,
        ) {
            actions.push(Action::RespondTrade { accept: true });
        }
        if self.trade_negotiation_round < 1 {
            actions.extend(self.generated_counteroffers(trade));
        }
        actions
    }

    fn generated_counteroffers(&self, trade: TradeOffer) -> Vec<Action> {
        let actor = self.trade_cursor;
        let hand = self.players[actor as usize].resources;
        let mut give_options = vec![trade.receive];
        let mut receive_options = vec![trade.give];
        for resource in Resource::ALL {
            if hand[resource.index()] > trade.receive[resource.index()] {
                let mut give = trade.receive;
                give[resource.index()] = give[resource.index()].saturating_add(1);
                if give.iter().copied().sum::<u8>() <= 2 {
                    give_options.push(give);
                }
            }
            // A counteroffer may request a card without knowing whether the
            // original creator currently holds it. The creator's later legal
            // response resolves that uncertainty. Conditioning this action
            // list on their sampled hidden hand leaks private information into
            // an information-set search.
            let mut receive = trade.give;
            receive[resource.index()] = receive[resource.index()].saturating_add(1);
            if receive.iter().copied().sum::<u8>() <= 2 {
                receive_options.push(receive);
            }
        }
        // Include one-edit substitutions, not only "add one more" demands.
        // These form the local counteroffer neighborhood around the incoming
        // bundle while keeping the action space bounded.
        for resource in Resource::ALL {
            if hand[resource.index()] > 0 {
                let mut give = [0; 5];
                give[resource.index()] = 1;
                give_options.push(give);
            }
            let mut receive = [0; 5];
            receive[resource.index()] = 1;
            receive_options.push(receive);
        }
        give_options.sort_unstable();
        give_options.dedup();
        receive_options.sort_unstable();
        receive_options.dedup();
        let mut actions = Vec::new();
        for give in give_options {
            for receive in &receive_options {
                if give == trade.receive && *receive == trade.give {
                    continue;
                }
                if give.iter().copied().sum::<u8>() == 0
                    || receive.iter().copied().sum::<u8>() == 0
                    || !contains(&hand, &give)
                    || Resource::ALL
                        .iter()
                        .any(|resource| give[resource.index()] > 0 && receive[resource.index()] > 0)
                {
                    continue;
                }
                actions.push(Action::CounterTrade {
                    give,
                    receive: *receive,
                });
            }
        }
        let hand_score = |candidate: ResourceHand| {
            let ready = [ROAD_COST, SETTLEMENT_COST, CITY_COST, DEVELOPMENT_COST]
                .iter()
                .enumerate()
                .filter(|(_, cost)| contains(&candidate, cost))
                .map(|(kind, _)| [0.28, 1.42, 1.26, 0.72][kind])
                .fold(0.0_f32, f32::max);
            let near = [ROAD_COST, SETTLEMENT_COST, CITY_COST, DEVELOPMENT_COST]
                .iter()
                .enumerate()
                .map(|(kind, cost)| {
                    let missing = cost
                        .iter()
                        .zip(candidate)
                        .map(|(required, available)| required.saturating_sub(available) as f32)
                        .sum::<f32>();
                    [0.24, 1.08, 0.98, 0.56][kind] / (1.0 + missing)
                })
                .fold(0.0_f32, f32::max);
            let weighted = candidate
                .iter()
                .zip([0.98_f32, 0.98, 0.73, 1.22, 1.10])
                .map(|(amount, weight)| *amount as f32 * weight)
                .sum::<f32>()
                * 0.11;
            let overflow = candidate.iter().copied().sum::<u8>().saturating_sub(7) as f32;
            ready + near + weighted - overflow * overflow * 0.045
        };
        let before = hand_score(hand);
        let creator_threat = self.players[trade.creator as usize].public_victory_points as f32
            / self.victory_target.max(1) as f32;
        actions.sort_by(|left, right| {
            let score = |action: &Action| {
                let Action::CounterTrade { give, receive } = action else {
                    return f32::NEG_INFINITY;
                };
                let mut after = hand;
                for resource in 0..5 {
                    after[resource] = after[resource]
                        .saturating_sub(give[resource])
                        .saturating_add(receive[resource]);
                }
                let feeds_creator = give[Resource::Grain.index()] as f32 * 1.25
                    + give[Resource::Ore.index()] as f32 * 1.15
                    + (give[Resource::Lumber.index()] + give[Resource::Brick.index()]) as f32
                        * 0.68;
                let denies_creator = receive[Resource::Grain.index()] as f32 * 0.34
                    + receive[Resource::Ore.index()] as f32 * 0.30;
                hand_score(after) - before - feeds_creator * creator_threat * 0.46
                    + denies_creator * creator_threat
            };
            score(right)
                .total_cmp(&score(left))
                .then_with(|| format!("{left:?}").cmp(&format!("{right:?}")))
        });
        // Keep a broad but bounded neighborhood. The strategic layer applies
        // progressive widening; this core cap must still preserve substitutions
        // across all five resource types instead of starving them before
        // search can rank them.
        actions.truncate(24);
        actions
    }

    fn respond_trade(&mut self, accept: bool) -> Result<(), RuleError> {
        if self.phase != Phase::TradeResponses {
            return Err(RuleError::WrongPhase);
        }
        let mut trade = self.trade.ok_or(RuleError::InvalidTrade)?;
        if self.trade_responses_complete(trade) || trade.recipients & (1 << self.trade_cursor) == 0
        {
            return Err(RuleError::WrongActor);
        }
        if accept
            && !contains(
                &self.players[self.trade_cursor as usize].resources,
                &trade.receive,
            )
        {
            return Err(RuleError::InsufficientResources);
        }
        if accept {
            trade.accepted |= 1 << self.trade_cursor;
        } else {
            trade.rejected |= 1 << self.trade_cursor;
        }
        self.trade = Some(trade);
        if !self.trade_responses_complete(trade) {
            self.trade_cursor = self
                .next_trade_recipient(trade.recipients, self.trade_cursor + 1)
                .ok_or(RuleError::InvalidTrade)?;
        }
        Ok(())
    }

    fn counter_trade(
        &mut self,
        give: ResourceHand,
        receive: ResourceHand,
    ) -> Result<(), RuleError> {
        if self.phase != Phase::TradeResponses || self.trade_negotiation_round >= 1 {
            return Err(RuleError::WrongPhase);
        }
        let previous = self.trade.ok_or(RuleError::InvalidTrade)?;
        let actor = self.trade_cursor;
        if self.trade_responses_complete(previous)
            || !contains(&self.players[actor as usize].resources, &give)
            || give.iter().copied().sum::<u8>() == 0
            || receive.iter().copied().sum::<u8>() == 0
            || Resource::ALL
                .iter()
                .any(|resource| give[resource.index()] > 0 && receive[resource.index()] > 0)
        {
            return Err(RuleError::InvalidTrade);
        }
        self.trade = Some(TradeOffer {
            creator: actor,
            recipients: 1 << previous.creator,
            give,
            receive,
            accepted: 0,
            rejected: 0,
        });
        self.trade_cursor = previous.creator;
        self.trade_negotiation_round += 1;
        Ok(())
    }

    fn confirm_trade(&mut self, partner: u8) -> Result<(), RuleError> {
        if self.phase != Phase::TradeResponses {
            return Err(RuleError::WrongPhase);
        }
        let trade = self.trade.ok_or(RuleError::InvalidTrade)?;
        if !self.trade_responses_complete(trade)
            || trade.accepted & (1 << partner) == 0
            || !contains(&self.players[trade.creator as usize].resources, &trade.give)
            || !contains(&self.players[partner as usize].resources, &trade.receive)
        {
            return Err(RuleError::InvalidTrade);
        }
        subtract(
            &mut self.players[trade.creator as usize].resources,
            &trade.give,
        );
        add(
            &mut self.players[trade.creator as usize].resources,
            &trade.receive,
        );
        subtract(
            &mut self.players[partner as usize].resources,
            &trade.receive,
        );
        add(&mut self.players[partner as usize].resources, &trade.give);
        self.trade = None;
        self.last_rejected_trade = None;
        self.trade_negotiation_round = 0;
        self.phase = Phase::Main;
        Ok(())
    }

    fn cancel_trade(&mut self) -> Result<(), RuleError> {
        if self.phase != Phase::TradeResponses
            || !self
                .trade
                .is_some_and(|trade| self.trade_responses_complete(trade))
        {
            return Err(RuleError::WrongPhase);
        }
        self.last_rejected_trade = self.trade;
        self.trade = None;
        self.trade_negotiation_round = 0;
        self.phase = Phase::Main;
        Ok(())
    }

    fn trade_responses_complete(&self, trade: TradeOffer) -> bool {
        (trade.accepted | trade.rejected) & trade.recipients == trade.recipients
    }

    fn next_trade_recipient(&self, recipients: u8, start: u8) -> Option<u8> {
        (start..self.board.num_players).find(|player| recipients & (1 << player) != 0)
    }

    fn end_turn(&mut self) -> Result<(), RuleError> {
        if self.phase != Phase::Main {
            return Err(RuleError::WrongPhase);
        }
        let player = &mut self.players[self.current_player as usize];
        player.bought_development = [0; 5];
        player.played_development_this_turn = false;
        self.domestic_trade_used = false;
        self.domestic_trade_count = 0;
        self.last_rejected_trade = None;
        self.trade_negotiation_round = 0;
        self.current_player = (self.current_player + 1) % self.board.num_players;
        self.turn += 1;
        self.last_roll = 0;
        self.phase = Phase::PreRoll;
        Ok(())
    }

    fn pay_current(&mut self, cost: &ResourceHand) -> Result<(), RuleError> {
        let hand = &mut self.players[self.current_player as usize].resources;
        if !contains(hand, cost) {
            return Err(RuleError::InsufficientResources);
        }
        subtract(hand, cost);
        add(&mut self.bank, cost);
        Ok(())
    }

    pub fn longest_road_length(&self, player: u8) -> u8 {
        fn walk(state: &GameState, player: u8, edge: u8, through: u8, used: u128) -> u8 {
            let edge_bit = 1u128 << edge;
            if used & edge_bit != 0 || state.roads[edge as usize] != Some(player) {
                return 0;
            }
            let used = used | edge_bit;
            let [a, b] = state.board.edges[edge as usize].vertices;
            let next_vertex = if a == through { b } else { a };
            if state.buildings[next_vertex as usize]
                .is_some_and(|building| building.player() != player)
            {
                return 1;
            }
            let tail = state.board.vertices[next_vertex as usize]
                .adjacent_edges
                .iter()
                .copied()
                .filter(|next| *next != edge)
                .map(|next| walk(state, player, next, next_vertex, used))
                .max()
                .unwrap_or(0);
            1 + tail
        }

        self.roads
            .iter()
            .enumerate()
            .filter(|(_, owner)| **owner == Some(player))
            .map(|(edge, _)| {
                let [a, b] = self.board.edges[edge].vertices;
                walk(self, player, edge as u8, a, 0).max(walk(self, player, edge as u8, b, 0))
            })
            .max()
            .unwrap_or(0)
    }

    fn update_longest_road(&mut self) {
        let lengths = (0..self.board.num_players)
            .map(|player| self.longest_road_length(player))
            .collect::<Vec<_>>();
        let best = lengths.iter().copied().max().unwrap_or(0);
        let leaders = lengths
            .iter()
            .enumerate()
            .filter(|(_, length)| **length == best && best >= 5)
            .map(|(player, _)| player as u8)
            .collect::<Vec<_>>();
        let next = if let Some(holder) = self.longest_road_holder
            && leaders.contains(&holder)
        {
            Some(holder)
        } else if leaders.len() == 1 {
            Some(leaders[0])
        } else {
            None
        };
        if next == self.longest_road_holder {
            return;
        }
        if let Some(old) = self.longest_road_holder {
            self.players[old as usize].has_longest_road = false;
            self.players[old as usize].public_victory_points -= 2;
        }
        if let Some(new) = next {
            self.players[new as usize].has_longest_road = true;
            self.players[new as usize].public_victory_points += 2;
        }
        self.longest_road_holder = next;
    }

    fn update_largest_army(&mut self) {
        let best = self
            .players
            .iter()
            .map(|player| player.played_knights)
            .max()
            .unwrap_or(0);
        let leaders = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, player)| player.played_knights == best && best >= 3)
            .map(|(player, _)| player as u8)
            .collect::<Vec<_>>();
        let next = if let Some(holder) = self.largest_army_holder
            && leaders.contains(&holder)
        {
            Some(holder)
        } else if leaders.len() == 1 {
            Some(leaders[0])
        } else {
            None
        };
        if next == self.largest_army_holder {
            return;
        }
        if let Some(old) = self.largest_army_holder {
            self.players[old as usize].has_largest_army = false;
            self.players[old as usize].public_victory_points -= 2;
        }
        if let Some(new) = next {
            self.players[new as usize].has_largest_army = true;
            self.players[new as usize].public_victory_points += 2;
        }
        self.largest_army_holder = next;
    }
}

fn contains(hand: &ResourceHand, cost: &ResourceHand) -> bool {
    hand.iter()
        .zip(cost)
        .all(|(available, required)| available >= required)
}

fn add(hand: &mut ResourceHand, cards: &ResourceHand) {
    for index in 0..5 {
        hand[index] += cards[index];
    }
}

fn subtract(hand: &mut ResourceHand, cards: &ResourceHand) {
    for index in 0..5 {
        hand[index] -= cards[index];
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Action, Building, DevCard, GameState, NodeKind, Phase, PlayerState, Resource, SplitMix64,
        TradeOffer,
    };

    fn play_setup(state: &mut GameState) {
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions().into_iter().next().unwrap();
            state.apply(&action).unwrap();
        }
    }

    #[test]
    fn setup_snakes_and_grants_second_settlement_resources() {
        let mut state = GameState::standard(7, 4);
        let mut placement_order = Vec::new();
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            if state.phase == Phase::SetupSettlement {
                placement_order.push(state.current_player);
            }
            let action = state.legal_actions().into_iter().next().unwrap();
            state.apply(&action).unwrap();
        }
        assert_eq!(placement_order, [0, 1, 2, 3, 3, 2, 1, 0]);
        assert_eq!(state.current_player, 0);
        assert_eq!(state.phase, Phase::PreRoll);
        assert!(state.players.iter().all(|player| {
            player.public_victory_points == 2
                && player.roads_left == 13
                && player.settlements_left == 3
        }));
        assert!(
            state
                .players
                .iter()
                .all(|player| (1..=3).contains(&player.resource_total()))
        );
        let cards_in_hands = state
            .players
            .iter()
            .map(PlayerState::resource_total)
            .sum::<u8>();
        let cards_missing_from_bank = state
            .bank
            .iter()
            .map(|remaining| 19 - remaining)
            .sum::<u8>();
        assert_eq!(cards_in_hands, cards_missing_from_bank);
    }

    #[test]
    fn production_respects_city_and_robber() {
        let mut state = GameState::standard(11, 3);
        play_setup(&mut state);
        let target = state
            .buildings
            .iter()
            .position(|building| *building == Some(Building::Settlement(0)))
            .unwrap();
        let hex = state.board.vertices[target].adjacent_hexes[0];
        let tile = state.board.hexes[hex as usize].clone();
        let Some(resource) = tile.resource else {
            return;
        };
        state.buildings[target] = Some(Building::City(0));
        state.robber_hex = state
            .board
            .hexes
            .iter()
            .position(|candidate| candidate.number != tile.number)
            .unwrap() as u8;
        let before = state.players[0].resources[resource.index()];
        state.apply(&Action::Roll).unwrap();
        state
            .apply(&Action::ResolveRoll { value: tile.number })
            .unwrap();
        assert!(state.players[0].resources[resource.index()] >= before + 2);
    }

    #[test]
    fn domestic_trade_requires_receiver_resources_and_confirmation() {
        let mut state = GameState::standard(3, 3);
        play_setup(&mut state);
        state.phase = Phase::Main;
        state.players[0].resources = [1, 0, 0, 0, 0];
        state.players[1].resources = [0, 1, 0, 0, 0];
        state
            .apply(&Action::OfferTrade {
                recipients: 0b010,
                give: [1, 0, 0, 0, 0],
                receive: [0, 1, 0, 0, 0],
            })
            .unwrap();
        assert_eq!(state.actor(), 1);
        state.apply(&Action::RespondTrade { accept: true }).unwrap();
        assert_eq!(state.actor(), 0);
        state.apply(&Action::ConfirmTrade { partner: 1 }).unwrap();
        assert_eq!(state.players[0].resources, [0, 1, 0, 0, 0]);
        assert_eq!(state.players[1].resources, [1, 0, 0, 0, 0]);
        assert_eq!(state.phase, Phase::Main);
        assert!(state.domestic_trade_used);
        assert_eq!(state.domestic_trade_count, 1);
        assert!(
            state
                .legal_actions()
                .iter()
                .any(|action| matches!(action, Action::OfferTrade { .. }))
        );
        state.apply(&Action::EndTurn).unwrap();
        assert!(!state.domestic_trade_used);
        assert_eq!(state.domestic_trade_count, 0);
    }

    #[test]
    fn main_phase_generates_bounded_legal_domestic_offers() {
        let mut state = GameState::standard(19, 4);
        play_setup(&mut state);
        state.phase = Phase::Main;
        state.bank[Resource::Lumber.index()] -= 2;
        state.players[0].resources[Resource::Lumber.index()] += 2;
        let offers = state
            .legal_actions()
            .into_iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::OfferTrade { give, .. }
                        if give[Resource::Lumber.index()] > 0
                )
            })
            .collect::<Vec<_>>();
        assert!(!offers.is_empty());
        assert!(offers.len() <= 96);
        assert!(offers.iter().any(|action| matches!(
            action,
            Action::OfferTrade { give, .. }
                if give.iter().copied().sum::<u8>() == 1
        )));
        assert!(offers.iter().any(|action| matches!(
            action,
            Action::OfferTrade { give, .. }
                if give.iter().copied().sum::<u8>() == 2
        )));
        for offer in offers {
            let mut next = state.clone();
            next.apply(&offer).unwrap();
            assert_eq!(next.phase, Phase::TradeResponses);
        }
    }

    #[test]
    fn domestic_offers_can_complete_a_road_and_settlement_plan() {
        let mut state = GameState::standard(29, 4);
        play_setup(&mut state);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [2, 2, 2, 0, 0];
        let offer = state.legal_actions().into_iter().find(|action| {
            matches!(
                action,
                Action::OfferTrade { give, receive, .. }
                    if give == &[0, 0, 1, 0, 0]
                        && receive == &[0, 0, 0, 1, 0]
            )
        });
        assert!(
            offer.is_some(),
            "plan-derived offers must include wool for the missing grain"
        );
    }

    #[test]
    fn plan_derived_mixed_and_one_for_two_offers_survive_the_candidate_cap() {
        let mut state = GameState::standard(30, 4);
        play_setup(&mut state);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [2, 2, 0, 0, 0];
        let offers = state.legal_actions();
        assert!(offers.iter().any(|action| {
            matches!(
                action,
                Action::OfferTrade { give, receive, .. }
                    if give.iter().copied().sum::<u8>() == 1
                        && receive[Resource::Wool.index()] == 1
                        && receive[Resource::Grain.index()] == 1
            )
        }));
        assert!(offers.iter().any(|action| {
            matches!(
                action,
                Action::OfferTrade { give, receive, .. }
                    if give.iter().copied().sum::<u8>() == 2
                        && receive.iter().copied().sum::<u8>() == 2
            )
        }));
    }

    #[test]
    fn counteroffers_include_one_resource_substitutions() {
        let mut state = GameState::standard(31, 4);
        play_setup(&mut state);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [1, 1, 1, 1, 0];
        state.players[1].resources = [1, 1, 1, 1, 0];
        state
            .apply(&Action::OfferTrade {
                recipients: 1 << 1,
                give: [1, 0, 0, 0, 0],
                receive: [0, 1, 0, 0, 0],
            })
            .unwrap();
        assert!(state.legal_actions().iter().any(|action| {
            matches!(
                action,
                Action::CounterTrade { give, receive }
                    if give == &[0, 0, 1, 0, 0]
                        && receive == &[1, 0, 0, 0, 0]
            )
        }));
    }

    #[test]
    fn one_counter_round_ends_the_negotiation_loop() {
        let mut state = GameState::standard(32, 3);
        play_setup(&mut state);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [2, 2, 2, 2, 0];
        state.players[1].resources = [2, 2, 2, 2, 0];
        state
            .apply(&Action::OfferTrade {
                recipients: 1 << 1,
                give: [1, 0, 0, 0, 0],
                receive: [0, 1, 0, 0, 0],
            })
            .unwrap();
        let counter = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::CounterTrade { .. }))
            .expect("the first recipient may make one bounded counteroffer");
        state.apply(&counter).unwrap();
        assert_eq!(state.actor(), 0);
        assert!(
            state
                .legal_actions()
                .iter()
                .all(|action| !matches!(action, Action::CounterTrade { .. })),
            "the creator must accept or reject instead of entering a counter loop"
        );
    }

    #[test]
    fn counteroffer_actions_do_not_reveal_the_creators_hidden_resources() {
        let mut first = GameState::standard(33, 4);
        play_setup(&mut first);
        first.phase = Phase::Main;
        first.current_player = 0;
        first.players[0].resources = [1, 1, 1, 1, 0];
        first.players[1].resources = [1, 1, 1, 1, 0];
        first
            .apply(&Action::OfferTrade {
                recipients: 1 << 1,
                give: [1, 0, 0, 0, 0],
                receive: [0, 1, 0, 0, 0],
            })
            .unwrap();
        let mut second = first.clone();
        first.players[0].resources = [1, 0, 0, 0, 3];
        second.players[0].resources = [1, 0, 0, 3, 0];
        assert_eq!(first.observation_hash(1), second.observation_hash(1));
        assert_eq!(first.legal_actions(), second.legal_actions());
    }

    #[test]
    fn an_inconsistent_hidden_trade_particle_never_emits_an_invalid_confirmation() {
        let mut state = GameState::standard(35, 3);
        play_setup(&mut state);
        state.phase = Phase::TradeResponses;
        state.current_player = 0;
        state.players[0].resources = [0, 1, 0, 0, 0];
        state.players[1].resources = [0, 0, 0, 0, 0];
        state.trade = Some(TradeOffer {
            creator: 0,
            recipients: 1 << 1,
            give: [0, 1, 0, 0, 0],
            receive: [1, 0, 0, 0, 0],
            accepted: 1 << 1,
            rejected: 0,
        });
        state.trade_cursor = 1;
        let actions = state.legal_actions();
        assert!(actions.contains(&Action::CancelTrade));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::ConfirmTrade { .. })),
        );
        assert!(actions.iter().all(|action| {
            let mut next = state.clone();
            next.apply(action).is_ok()
        }));
    }

    #[test]
    fn seven_forces_exact_discards_before_robber() {
        let mut state = GameState::standard(17, 3);
        play_setup(&mut state);
        state.players[0].resources = [8, 0, 0, 0, 0];
        state.players[1].resources = [0, 3, 5, 0, 0];
        state.apply(&Action::Roll).unwrap();
        state.apply(&Action::ResolveRoll { value: 7 }).unwrap();
        assert_eq!(state.phase, Phase::Discard);
        assert_eq!(state.actor(), 0);
        state
            .apply(&Action::Discard {
                cards: [4, 0, 0, 0, 0],
            })
            .unwrap();
        assert_eq!(state.actor(), 1);
        state
            .apply(&Action::Discard {
                cards: [0, 1, 3, 0, 0],
            })
            .unwrap();
        assert_eq!(state.phase, Phase::MoveRobber);
    }

    #[test]
    fn port_ratios_come_from_owned_buildings() {
        let mut state = GameState::standard(23, 3);
        let port_vertex = state
            .board
            .vertices
            .iter()
            .position(|vertex| vertex.port.is_some())
            .unwrap();
        state.buildings[port_vertex] = Some(Building::Settlement(0));
        let ratios = state.trade_ratios(0);
        match state.board.vertices[port_vertex].port.unwrap() {
            crate::Port::Generic => assert_eq!(ratios, [3; 5]),
            crate::Port::Resource(resource) => {
                assert_eq!(ratios[resource.index()], 2);
                for other in Resource::ALL {
                    if other != resource {
                        assert_eq!(ratios[other.index()], 4);
                    }
                }
            }
        }
    }

    #[test]
    fn chance_nodes_use_true_outcome_weights() {
        let mut state = GameState::standard(91, 3);
        play_setup(&mut state);
        state.apply(&Action::Roll).unwrap();
        assert_eq!(state.node_kind(), NodeKind::Chance);
        assert_eq!(
            state
                .legal_actions()
                .iter()
                .map(|action| state.chance_weight(action))
                .sum::<u16>(),
            36
        );

        let mut rng = SplitMix64::new(5);
        for _ in 0..100 {
            assert!(matches!(
                state.sample_chance(&mut rng),
                Some(Action::ResolveRoll { value: 2..=12 })
            ));
        }
    }

    #[test]
    fn development_purchase_is_a_private_chance_transition() {
        let mut state = GameState::standard(19, 3);
        play_setup(&mut state);
        state.phase = Phase::Main;
        for resource in [Resource::Wool, Resource::Grain, Resource::Ore] {
            state.bank[resource.index()] -= 1;
            state.players[0].resources[resource.index()] += 1;
        }
        state.apply(&Action::BuyDevelopment).unwrap();
        assert_eq!(state.phase, Phase::DevelopmentChance);
        assert_eq!(
            state
                .legal_actions()
                .iter()
                .map(|action| state.chance_weight(action))
                .sum::<u16>(),
            25
        );
        state
            .apply(&Action::ResolveDevelopment {
                card: DevCard::Knight,
            })
            .unwrap();
        assert_eq!(state.players[0].development[DevCard::Knight.index()], 1);
        state.validate().unwrap();
    }

    #[test]
    fn road_building_respects_remaining_piece_inventory() {
        let mut state = GameState::standard(91, 4);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.buildings[0] = Some(Building::Settlement(0));
        state.players[0].settlements_left = 4;
        state.players[0].public_victory_points = 1;
        state.players[0].development[DevCard::RoadBuilding.index()] = 1;

        state.players[0].roads_left = 0;
        assert!(
            state
                .legal_actions()
                .iter()
                .all(|action| !matches!(action, Action::PlayRoadBuilding { .. }))
        );

        state.players[0].roads_left = 1;
        let actions = state
            .legal_actions()
            .into_iter()
            .filter(|action| matches!(action, Action::PlayRoadBuilding { .. }))
            .collect::<Vec<_>>();
        assert!(!actions.is_empty());
        assert!(
            actions
                .iter()
                .all(|action| matches!(action, Action::PlayRoadBuilding { second: None, .. }))
        );
        state.apply(&actions[0]).unwrap();
        assert_eq!(state.players[0].roads_left, 0);
    }

    #[test]
    fn failed_action_is_transactional() {
        let mut state = GameState::standard(13, 3);
        play_setup(&mut state);
        let before = state.clone();
        assert!(
            state
                .apply(&Action::BuildRoad {
                    edge: state.board.edges.len() as u8,
                })
                .is_err()
        );
        assert_eq!(state, before);
    }

    #[test]
    fn seeded_random_trace_preserves_invariants_and_hashes() {
        let mut first = GameState::standard(123, 4);
        let mut second = first.clone();
        let mut rng_first = SplitMix64::new(456);
        let mut rng_second = SplitMix64::new(456);
        for _ in 0..1_000 {
            assert_eq!(first.state_hash(), second.state_hash());
            first.validate().unwrap();
            second.validate().unwrap();
            if first.is_terminal() {
                break;
            }
            let action_first = if first.node_kind() == NodeKind::Chance {
                first.sample_chance(&mut rng_first).unwrap()
            } else {
                let actions = first.legal_actions();
                actions[rng_first.range(actions.len())].clone()
            };
            let action_second = if second.node_kind() == NodeKind::Chance {
                second.sample_chance(&mut rng_second).unwrap()
            } else {
                let actions = second.legal_actions();
                actions[rng_second.range(actions.len())].clone()
            };
            assert_eq!(action_first, action_second);
            first.apply(&action_first).unwrap();
            second.apply(&action_second).unwrap();
        }
        assert_eq!(first, second);
    }

    #[test]
    fn observation_hash_hides_opponent_card_identity_but_not_own_identity() {
        let mut first = GameState::standard(33, 4);
        play_setup(&mut first);
        let mut second = first.clone();
        first.players[1].resources = [2, 0, 0, 0, 0];
        second.players[1].resources = [0, 2, 0, 0, 0];
        assert_ne!(first.state_hash(), second.state_hash());
        assert_eq!(first.public_hash(), second.public_hash());
        assert_eq!(first.observation_hash(0), second.observation_hash(0));
        assert_ne!(first.observation_hash(1), second.observation_hash(1));

        first.players[0].development = [1, 0, 0, 0, 0];
        first.players[0].bought_development = [1, 0, 0, 0, 0];
        first.players[1].development = [0, 1, 0, 0, 0];
        first.players[1].bought_development = [0, 1, 0, 0, 0];
        first.development_deck = [10, 2, 1, 1, 1];
        first.bank = [7, 6, 5, 4, 3];
        first.bank_is_public = false;
        let observed = first.observed_state(0);
        assert_eq!(observed.players[0].development, [1, 0, 0, 0, 0]);
        assert_eq!(observed.players[0].bought_development, [1, 0, 0, 0, 0]);
        assert_eq!(observed.players[1].development, [1, 0, 0, 0, 0]);
        assert_eq!(observed.players[1].bought_development, [1, 0, 0, 0, 0]);
        assert_eq!(observed.development_deck, [15, 0, 0, 0, 0]);
        assert_eq!(observed.bank, [25, 0, 0, 0, 0]);
    }
}
