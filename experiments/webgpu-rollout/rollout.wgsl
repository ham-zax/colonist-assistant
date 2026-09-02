// H2 feasibility-only WebGPU rollout kernel.
//
// This deliberately preserves the production packed-state/root/reduction shape
// while pruning only player-trade/setup branches and simplifying dev-card policy
// selection. It is not a production replacement for sim.cu.

const MAX_PLAYERS: u32 = 4u;
const HEX_COUNT: u32 = 19u;
const VERTEX_COUNT: u32 = 54u;
const EDGE_COUNT: u32 = 72u;
const MAX_VERTEX_ADJACENCY: u32 = 3u;
const STATE_WORDS: u32 = 404u;
const ACTION_WORDS: u32 = 12u;
const ROOT_STATS_WORDS: u32 = 10u;

const STATE_NUM_PLAYERS: u32 = 0u;
const STATE_PHASE: u32 = 1u;
const STATE_PHASE_ARG: u32 = 2u;
const STATE_CURRENT_PLAYER: u32 = 3u;
const STATE_ROBBER_HEX: u32 = 4u;
const STATE_VICTORY_TARGET: u32 = 5u;
const STATE_DISCARD_LIMIT: u32 = 6u;
const STATE_BANK_PUBLIC: u32 = 7u;
const STATE_LONGEST_HOLDER: u32 = 8u;
const STATE_LARGEST_HOLDER: u32 = 9u;
const STATE_TURN: u32 = 10u;
const STATE_LAST_ROLL: u32 = 11u;
const STATE_FRIENDLY_ROBBER: u32 = 12u;
const STATE_SETUP_STEP: u32 = 13u;
const STATE_DISCARD_CURSOR: u32 = 14u;
const STATE_ROBBER_RETURN_PHASE: u32 = 15u;
const STATE_ROBBER_RETURN_ARG: u32 = 16u;
const STATE_FREE_ROADS: u32 = 17u;
const STATE_DOMESTIC_TRADE_USED: u32 = 18u;
const STATE_DOMESTIC_TRADE_COUNT: u32 = 19u;
const STATE_PLAYER_TRADES_ENABLED: u32 = 20u;
const STATE_TRADE_CURSOR: u32 = 21u;
const STATE_TRADE_NEGOTIATION_ROUND: u32 = 22u;
const STATE_TRADE: u32 = 23u;
const STATE_LAST_REJECTED_TRADE: u32 = 38u;
const STATE_BANK: u32 = 53u;
const STATE_DEVELOPMENT_DECK: u32 = 58u;
const STATE_PLAYED_DEVELOPMENT: u32 = 63u;
const STATE_DISCARD_REMAINING: u32 = 68u;
const STATE_HEX_RESOURCES: u32 = 72u;
const STATE_HEX_NUMBERS: u32 = 91u;
const STATE_PORTS: u32 = 110u;
const STATE_BUILDINGS: u32 = 164u;
const STATE_ROADS: u32 = 218u;
const STATE_PLAYERS: u32 = 290u;
const PLAYER_STRIDE: u32 = 28u;
const STATE_DOMESTIC_TRADE_DISABLED: u32 = 402u;
const STATE_DOMESTIC_TRADE_EMBARGOES: u32 = 403u;

const PLAYER_RESOURCES: u32 = 0u;
const PLAYER_DEVELOPMENT: u32 = 5u;
const PLAYER_BOUGHT_DEVELOPMENT: u32 = 10u;
const PLAYER_PUBLIC_VP: u32 = 15u;
const PLAYER_PLAYED_KNIGHTS: u32 = 16u;
const PLAYER_ROADS_LEFT: u32 = 17u;
const PLAYER_SETTLEMENTS_LEFT: u32 = 18u;
const PLAYER_CITIES_LEFT: u32 = 19u;
const PLAYER_HAS_LONGEST: u32 = 20u;
const PLAYER_HAS_LARGEST: u32 = 21u;
const PLAYER_PLAYED_DEVELOPMENT_THIS_TURN: u32 = 22u;
const PLAYER_POLICY_PROFILE: u32 = 23u;

const ACTION_TAG: u32 = 0u;
const ACTION_ARG0: u32 = 1u;
const ACTION_ROLL: u32 = 2u;
const ACTION_RESOLVE_ROLL: u32 = 3u;
const ACTION_DISCARD: u32 = 4u;
const ACTION_MOVE_ROBBER: u32 = 5u;
const ACTION_RESOLVE_STEAL: u32 = 6u;
const ACTION_BUILD_ROAD: u32 = 7u;
const ACTION_BUILD_SETTLEMENT: u32 = 8u;
const ACTION_BUILD_CITY: u32 = 9u;
const ACTION_BUY_DEVELOPMENT: u32 = 10u;
const ACTION_RESOLVE_DEVELOPMENT: u32 = 11u;
const ACTION_PLAY_KNIGHT: u32 = 12u;
const ACTION_PLAY_ROAD_BUILDING: u32 = 13u;
const ACTION_PLAY_YEAR_OF_PLENTY: u32 = 14u;
const ACTION_PLAY_MONOPOLY: u32 = 15u;
const ACTION_MARITIME_TRADE: u32 = 16u;
const ACTION_END_TURN: u32 = 17u;

const PHASE_SETUP_SETTLEMENT: u32 = 0u;
const PHASE_SETUP_ROAD: u32 = 1u;
const PHASE_PRE_ROLL: u32 = 2u;
const PHASE_ROLL_CHANCE: u32 = 3u;
const PHASE_DISCARD: u32 = 4u;
const PHASE_MOVE_ROBBER: u32 = 5u;
const PHASE_RESOLVE_STEAL: u32 = 6u;
const PHASE_MAIN: u32 = 7u;
const PHASE_DEVELOPMENT_CHANCE: u32 = 8u;
const PHASE_TRADE_RESPONSES: u32 = 9u;
const PHASE_FINISHED: u32 = 10u;

const STATUS_OK: u32 = 0u;
const STATUS_UNSUPPORTED_ACTION: u32 = 1u;
const STATUS_INVALID_PHASE: u32 = 2u;
const STATUS_INVALID_ACTION: u32 = 3u;

const TOPO_VERTEX_HEX_COUNTS: u32 = 0u;
const TOPO_VERTEX_HEXES: u32 = 54u;
const TOPO_VERTEX_VERTEX_COUNTS: u32 = 216u;
const TOPO_VERTEX_VERTICES: u32 = 270u;
const TOPO_VERTEX_EDGE_COUNTS: u32 = 432u;
const TOPO_VERTEX_EDGES: u32 = 486u;
const TOPO_EDGE_VERTICES: u32 = 648u;

struct Params {
  lane_count: u32,
  root_count: u32,
  chunk_rollouts: u32,
  total_rollouts: u32,
  rollout_offset: u32,
  base_stride: u32,
  step_count: u32,
  seed_lo: u32,
  seed_hi: u32,
  _pad0: u32,
  _pad1: u32,
  _pad2: u32,
}

struct U64 {
  lo: u32,
  hi: u32,
}

struct EdgeMask {
  a: u32,
  b: u32,
  c: u32,
}

struct RobberChoice {
  found: u32,
  hex: u32,
  victim_code: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> base_states: array<u32>;
@group(0) @binding(2) var<storage, read> topology: array<u32>;
@group(0) @binding(3) var<storage, read> root_data: array<u32>;
@group(0) @binding(4) var<storage, read_write> lane_data: array<u32>;
@group(0) @binding(5) var<storage, read_write> stats: array<atomic<u32>>;

fn action_base() -> u32 { return STATE_WORDS * params.lane_count; }
fn status_base() -> u32 { return action_base() + ACTION_WORDS * params.lane_count; }
fn rng_lo_base() -> u32 { return status_base() + params.lane_count; }
fn rng_hi_base() -> u32 { return rng_lo_base() + params.lane_count; }
fn chance_lo_base() -> u32 { return rng_hi_base() + params.lane_count; }
fn chance_hi_base() -> u32 { return chance_lo_base() + params.lane_count; }

fn state_get(field: u32, lane: u32) -> u32 {
  return lane_data[field * params.lane_count + lane];
}
fn state_set(field: u32, lane: u32, value: u32) {
  lane_data[field * params.lane_count + lane] = value;
}
fn base_get(field: u32, base: u32) -> u32 {
  return base_states[field * params.base_stride + base];
}
fn action_get(field: u32, lane: u32) -> u32 {
  return lane_data[action_base() + field * params.lane_count + lane];
}
fn action_set(field: u32, lane: u32, value: u32) {
  lane_data[action_base() + field * params.lane_count + lane] = value;
}
fn root_action_get(field: u32, root: u32) -> u32 {
  return root_data[field * params.root_count + root];
}
fn root_base_index(root: u32) -> u32 {
  return root_data[ACTION_WORDS * params.root_count + root];
}
fn status_get(lane: u32) -> u32 { return lane_data[status_base() + lane]; }
fn status_set(lane: u32, value: u32) { lane_data[status_base() + lane] = value; }

fn player_field(player: u32, offset: u32) -> u32 {
  return STATE_PLAYERS + player * PLAYER_STRIDE + offset;
}
fn player_get(lane: u32, player: u32, offset: u32) -> u32 {
  return state_get(player_field(player, offset), lane);
}
fn player_set(lane: u32, player: u32, offset: u32, value: u32) {
  state_set(player_field(player, offset), lane, value);
}
fn base_player_get(base: u32, player: u32, offset: u32) -> u32 {
  return base_get(player_field(player, offset), base);
}

fn topo_vertex_hex_count(vertex: u32) -> u32 { return topology[TOPO_VERTEX_HEX_COUNTS + vertex]; }
fn topo_vertex_hex(vertex: u32, slot: u32) -> u32 { return topology[TOPO_VERTEX_HEXES + vertex * 3u + slot]; }
fn topo_vertex_vertex_count(vertex: u32) -> u32 { return topology[TOPO_VERTEX_VERTEX_COUNTS + vertex]; }
fn topo_vertex_vertex(vertex: u32, slot: u32) -> u32 { return topology[TOPO_VERTEX_VERTICES + vertex * 3u + slot]; }
fn topo_vertex_edge_count(vertex: u32) -> u32 { return topology[TOPO_VERTEX_EDGE_COUNTS + vertex]; }
fn topo_vertex_edge(vertex: u32, slot: u32) -> u32 { return topology[TOPO_VERTEX_EDGES + vertex * 3u + slot]; }
fn topo_edge_vertex(edge: u32, endpoint: u32) -> u32 { return topology[TOPO_EDGE_VERTICES + edge * 2u + endpoint]; }

fn u64_xor(a: U64, b: U64) -> U64 { return U64(a.lo ^ b.lo, a.hi ^ b.hi); }
fn u64_add(a: U64, b: U64) -> U64 {
  let lo = a.lo + b.lo;
  let carry = select(0u, 1u, lo < a.lo);
  return U64(lo, a.hi + b.hi + carry);
}
fn u64_shr(a: U64, shift: u32) -> U64 {
  if (shift == 0u) { return a; }
  if (shift < 32u) {
    return U64((a.lo >> shift) | (a.hi << (32u - shift)), a.hi >> shift);
  }
  if (shift < 64u) { return U64(a.hi >> (shift - 32u), 0u); }
  return U64(0u, 0u);
}
fn mul32_wide(a: u32, b: u32) -> U64 {
  let a0 = a & 0xffffu;
  let a1 = a >> 16u;
  let b0 = b & 0xffffu;
  let b1 = b >> 16u;
  let p0 = a0 * b0;
  let p1 = a0 * b1;
  let p2 = a1 * b0;
  let p3 = a1 * b1;
  let middle = (p0 >> 16u) + (p1 & 0xffffu) + (p2 & 0xffffu);
  let lo = (p0 & 0xffffu) | ((middle & 0xffffu) << 16u);
  let hi = p3 + (p1 >> 16u) + (p2 >> 16u) + (middle >> 16u);
  return U64(lo, hi);
}
fn u64_mul(a: U64, b: U64) -> U64 {
  let low = mul32_wide(a.lo, b.lo);
  return U64(low.lo, low.hi + a.lo * b.hi + a.hi * b.lo);
}
fn u64_mod_u32(value: U64, divisor: u32) -> u32 {
  if (divisor <= 1u) { return 0u; }
  if (divisor <= 0xffffu) {
    var rem = 0u;
    rem = ((rem << 16u) + (value.hi >> 16u)) % divisor;
    rem = ((rem << 16u) + (value.hi & 0xffffu)) % divisor;
    rem = ((rem << 16u) + (value.lo >> 16u)) % divisor;
    rem = ((rem << 16u) + (value.lo & 0xffffu)) % divisor;
    return rem;
  }
  var rem = 0u;
  var bit = 64u;
  loop {
    if (bit == 0u) { break; }
    bit -= 1u;
    var next = 0u;
    if (bit >= 32u) {
      next = (value.hi >> (bit - 32u)) & 1u;
    } else {
      next = (value.lo >> bit) & 1u;
    }
    let doubled = rem + rem;
    rem = select(doubled, doubled - divisor, doubled >= divisor);
    rem = rem + next;
    if (rem >= divisor) { rem -= divisor; }
  }
  return rem;
}
fn mix_stream_seed(base_seed: U64, global_index: U64, domain: U64) -> U64 {
  let seed_index_mix = U64(0xde82ef95u, 0xd1342543u);
  let mix1 = U64(0x1ce4e5b9u, 0xbf58476du);
  let mix2 = U64(0x133111ebu, 0x94d049bbu);
  var value = u64_xor(u64_xor(base_seed, domain), u64_mul(global_index, seed_index_mix));
  value = u64_mul(u64_xor(value, u64_shr(value, 30u)), mix1);
  value = u64_mul(u64_xor(value, u64_shr(value, 27u)), mix2);
  return u64_xor(value, u64_shr(value, 31u));
}
fn splitmix64_next(state: ptr<function, U64>) -> U64 {
  let gamma = U64(0x7f4a7c15u, 0x9e3779b9u);
  let mix1 = U64(0x1ce4e5b9u, 0xbf58476du);
  let mix2 = U64(0x133111ebu, 0x94d049bbu);
  *state = u64_add(*state, gamma);
  var value = *state;
  value = u64_mul(u64_xor(value, u64_shr(value, 30u)), mix1);
  value = u64_mul(u64_xor(value, u64_shr(value, 27u)), mix2);
  return u64_xor(value, u64_shr(value, 31u));
}
fn rng_range(state: ptr<function, U64>, end: u32) -> u32 {
  if (end <= 1u) { return 0u; }
  return u64_mod_u32(splitmix64_next(state), end);
}
fn load_rng(lane: u32, chance: bool) -> U64 {
  if (chance) { return U64(lane_data[chance_lo_base() + lane], lane_data[chance_hi_base() + lane]); }
  return U64(lane_data[rng_lo_base() + lane], lane_data[rng_hi_base() + lane]);
}
fn store_rng(lane: u32, chance: bool, value: U64) {
  if (chance) {
    lane_data[chance_lo_base() + lane] = value.lo;
    lane_data[chance_hi_base() + lane] = value.hi;
  } else {
    lane_data[rng_lo_base() + lane] = value.lo;
    lane_data[rng_hi_base() + lane] = value.hi;
  }
}

fn clear_action(lane: u32) {
  var field = 0u;
  loop {
    if (field >= ACTION_WORDS) { break; }
    action_set(field, lane, 0u);
    field += 1u;
  }
}
fn write_action(lane: u32, tag: u32, arg0: u32, arg1: u32, arg2: u32) {
  clear_action(lane);
  action_set(ACTION_TAG, lane, tag);
  action_set(ACTION_ARG0, lane, arg0);
  action_set(ACTION_ARG0 + 1u, lane, arg1);
  action_set(ACTION_ARG0 + 2u, lane, arg2);
}
fn weighted_select(rng: ptr<function, U64>, total: ptr<function, u32>, weight: u32) -> bool {
  if (weight == 0u) { return false; }
  let next = *total + weight;
  let selected = rng_range(rng, next) < weight;
  *total = next;
  return selected;
}

fn building_player(building: u32) -> u32 {
  if (building == 0u) { return 0xffffffffu; }
  if (building <= 4u) { return building - 1u; }
  return building - 5u;
}
fn building_multiplier(building: u32) -> u32 {
  if (building == 0u) { return 0u; }
  return select(2u, 1u, building <= 4u);
}
fn resource_total(lane: u32, player: u32) -> u32 {
  var total = 0u;
  var res = 0u;
  loop {
    if (res >= 5u) { break; }
    total += player_get(lane, player, PLAYER_RESOURCES + res);
    res += 1u;
  }
  return total;
}
fn pips_for_number(number: u32) -> u32 {
  if (number == 2u || number == 12u) { return 1u; }
  if (number == 3u || number == 11u) { return 2u; }
  if (number == 4u || number == 10u) { return 3u; }
  if (number == 5u || number == 9u) { return 4u; }
  if (number == 6u || number == 8u) { return 5u; }
  return 0u;
}
fn vertex_policy_score(lane: u32, vertex: u32) -> u32 {
  var score = 50u;
  var resource_mask = 0u;
  let count = topo_vertex_hex_count(vertex);
  var slot = 0u;
  loop {
    if (slot >= count) { break; }
    let hex = topo_vertex_hex(vertex, slot);
    score += pips_for_number(state_get(STATE_HEX_NUMBERS + hex, lane)) * 24u;
    let encoded = state_get(STATE_HEX_RESOURCES + hex, lane);
    if (encoded > 0u) { resource_mask |= 1u << (encoded - 1u); }
    slot += 1u;
  }
  var res = 0u;
  loop {
    if (res >= 5u) { break; }
    if ((resource_mask & (1u << res)) != 0u) { score += 18u; }
    res += 1u;
  }
  if (state_get(STATE_PORTS + vertex, lane) != 0u) { score += 25u; }
  return score;
}
fn road_policy_score(lane: u32, edge: u32) -> u32 {
  var best = 50u;
  var endpoint = 0u;
  loop {
    if (endpoint >= 2u) { break; }
    let vertex = topo_edge_vertex(edge, endpoint);
    if (state_get(STATE_BUILDINGS + vertex, lane) == 0u) {
      best = max(best, vertex_policy_score(lane, vertex));
    }
    let count = topo_vertex_vertex_count(vertex);
    var slot = 0u;
    loop {
      if (slot >= count) { break; }
      let next = topo_vertex_vertex(vertex, slot);
      if (state_get(STATE_BUILDINGS + next, lane) == 0u) {
        best = max(best, vertex_policy_score(lane, next) / 2u);
      }
      slot += 1u;
    }
    endpoint += 1u;
  }
  return best;
}
fn profile_scaled_weight(lane: u32, player: u32, profile_index: u32, base: u32) -> u32 {
  let profile = min(player_get(lane, player, PLAYER_POLICY_PROFILE + profile_index), 102u);
  return (base * (64u + profile)) / 115u;
}

fn cost_value(kind: u32, res: u32) -> u32 {
  if (kind == 0u) { return select(0u, 1u, res < 2u); }
  if (kind == 1u) { return select(0u, 1u, res < 4u); }
  if (kind == 2u) {
    if (res == 3u) { return 2u; }
    if (res == 4u) { return 3u; }
    return 0u;
  }
  if (kind == 3u) { return select(0u, 1u, res >= 2u); }
  return 0u;
}
fn has_cost(lane: u32, player: u32, kind: u32) -> bool {
  var res = 0u;
  loop {
    if (res >= 5u) { break; }
    if (player_get(lane, player, PLAYER_RESOURCES + res) < cost_value(kind, res)) { return false; }
    res += 1u;
  }
  return true;
}
fn pay_cost(lane: u32, kind: u32) -> bool {
  let player = state_get(STATE_CURRENT_PLAYER, lane);
  if (!has_cost(lane, player, kind)) { return false; }
  var res = 0u;
  loop {
    if (res >= 5u) { break; }
    let cost = cost_value(kind, res);
    player_set(lane, player, PLAYER_RESOURCES + res, player_get(lane, player, PLAYER_RESOURCES + res) - cost);
    state_set(STATE_BANK + res, lane, state_get(STATE_BANK + res, lane) + cost);
    res += 1u;
  }
  return true;
}
fn trade_ratio(lane: u32, player: u32, res: u32) -> u32 {
  var ratio = 4u;
  var vertex = 0u;
  loop {
    if (vertex >= VERTEX_COUNT) { break; }
    if (building_player(state_get(STATE_BUILDINGS + vertex, lane)) == player) {
      let port = state_get(STATE_PORTS + vertex, lane);
      if (port == 1u) { ratio = min(ratio, 3u); }
      if (port == res + 2u) { ratio = 2u; }
    }
    vertex += 1u;
  }
  return ratio;
}
fn maritime_policy_score(lane: u32, player: u32, give: u32, receive: u32, ratio: u32) -> u32 {
  var hand: array<u32, 5>;
  var res = 0u;
  loop {
    if (res >= 5u) { break; }
    hand[res] = player_get(lane, player, PLAYER_RESOURCES + res);
    res += 1u;
  }
  hand[give] -= ratio;
  hand[receive] += 1u;
  var score = 150u;
  if (receive == 3u) { score += 90u; }
  else if (receive == 4u) { score += 80u; }
  else if (receive < 2u) { score += 55u; }
  else { score += 45u; }
  var can_road = true;
  var can_settlement = true;
  var can_city = true;
  var can_development = true;
  res = 0u;
  loop {
    if (res >= 5u) { break; }
    can_road = can_road && hand[res] >= cost_value(0u, res);
    can_settlement = can_settlement && hand[res] >= cost_value(1u, res);
    can_city = can_city && hand[res] >= cost_value(2u, res);
    can_development = can_development && hand[res] >= cost_value(3u, res);
    res += 1u;
  }
  if (can_city) { score += 700u; }
  if (can_settlement) { score += 620u; }
  if (can_development) { score += 320u; }
  if (can_road) { score += 180u; }
  return score;
}

fn can_place_settlement(lane: u32, vertex: u32) -> bool {
  if (vertex >= VERTEX_COUNT || state_get(STATE_BUILDINGS + vertex, lane) != 0u) { return false; }
  let adjacent_vertices = topo_vertex_vertex_count(vertex);
  var slot = 0u;
  loop {
    if (slot >= adjacent_vertices) { break; }
    if (state_get(STATE_BUILDINGS + topo_vertex_vertex(vertex, slot), lane) != 0u) { return false; }
    slot += 1u;
  }
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  let adjacent_edges = topo_vertex_edge_count(vertex);
  slot = 0u;
  loop {
    if (slot >= adjacent_edges) { break; }
    if (state_get(STATE_ROADS + topo_vertex_edge(vertex, slot), lane) == current + 1u) { return true; }
    slot += 1u;
  }
  return false;
}
fn road_owner_with_extra(lane: u32, edge: u32, extra_edge: u32, current: u32) -> u32 {
  if (edge == extra_edge) { return current + 1u; }
  return state_get(STATE_ROADS + edge, lane);
}
fn can_build_road(lane: u32, edge: u32, extra_edge: u32) -> bool {
  if (edge >= EDGE_COUNT || edge == extra_edge || state_get(STATE_ROADS + edge, lane) != 0u) { return false; }
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  var endpoint = 0u;
  loop {
    if (endpoint >= 2u) { break; }
    let vertex = topo_edge_vertex(edge, endpoint);
    let building = state_get(STATE_BUILDINGS + vertex, lane);
    if (building != 0u) {
      if (building_player(building) == current) { return true; }
    } else {
      let count = topo_vertex_edge_count(vertex);
      var slot = 0u;
      loop {
        if (slot >= count) { break; }
        let neighbor = topo_vertex_edge(vertex, slot);
        if (neighbor != edge && road_owner_with_extra(lane, neighbor, extra_edge, current) == current + 1u) { return true; }
        slot += 1u;
      }
    }
    endpoint += 1u;
  }
  return false;
}

fn edge_used(mask: EdgeMask, edge: u32) -> bool {
  let bit = 1u << (edge & 31u);
  if (edge < 32u) { return (mask.a & bit) != 0u; }
  if (edge < 64u) { return (mask.b & bit) != 0u; }
  return (mask.c & bit) != 0u;
}
fn edge_mark(mask: EdgeMask, edge: u32, used: bool) -> EdgeMask {
  var next = mask;
  let bit = 1u << (edge & 31u);
  if (edge < 32u) { next.a = select(next.a & ~bit, next.a | bit, used); }
  else if (edge < 64u) { next.b = select(next.b & ~bit, next.b | bit, used); }
  else { next.c = select(next.c & ~bit, next.c | bit, used); }
  return next;
}
fn longest_road_from(lane: u32, player: u32, root_edge: u32, root_through: u32) -> u32 {
  var edge_stack: array<u32, 15>;
  var through_stack: array<u32, 15>;
  var next_slot: array<u32, 15>;
  var depth = 0i;
  var used = EdgeMask(0u, 0u, 0u);
  edge_stack[0] = root_edge;
  through_stack[0] = root_through;
  next_slot[0] = 0u;
  used = edge_mark(used, root_edge, true);
  var best = 1u;
  loop {
    if (depth < 0i) { break; }
    let d = u32(depth);
    let edge = edge_stack[d];
    let a = topo_edge_vertex(edge, 0u);
    let b = topo_edge_vertex(edge, 1u);
    let next_vertex = select(a, b, a == through_stack[d]);
    let owner = building_player(state_get(STATE_BUILDINGS + next_vertex, lane));
    if (owner != 0xffffffffu && owner != player) {
      used = edge_mark(used, edge, false);
      depth -= 1i;
      continue;
    }
    let count = topo_vertex_edge_count(next_vertex);
    var pushed = false;
    var slot = next_slot[d];
    loop {
      if (slot >= count) { break; }
      next_slot[d] = slot + 1u;
      let candidate = topo_vertex_edge(next_vertex, slot);
      if (candidate != edge && !edge_used(used, candidate) && state_get(STATE_ROADS + candidate, lane) == player + 1u && depth + 1i < 15i) {
        depth += 1i;
        let nd = u32(depth);
        edge_stack[nd] = candidate;
        through_stack[nd] = next_vertex;
        next_slot[nd] = 0u;
        used = edge_mark(used, candidate, true);
        best = max(best, nd + 1u);
        pushed = true;
        break;
      }
      slot += 1u;
    }
    if (pushed) { continue; }
    used = edge_mark(used, edge, false);
    depth -= 1i;
  }
  return best;
}
fn longest_road_length(lane: u32, player: u32) -> u32 {
  var best = 0u;
  var edge = 0u;
  loop {
    if (edge >= EDGE_COUNT) { break; }
    if (state_get(STATE_ROADS + edge, lane) == player + 1u) {
      let a = topo_edge_vertex(edge, 0u);
      let b = topo_edge_vertex(edge, 1u);
      best = max(best, max(longest_road_from(lane, player, edge, a), longest_road_from(lane, player, edge, b)));
    }
    edge += 1u;
  }
  return best;
}
fn update_longest_road(lane: u32) {
  let players = state_get(STATE_NUM_PLAYERS, lane);
  var lengths: array<u32, 4>;
  var best = 0u;
  var player = 0u;
  loop {
    if (player >= players) { break; }
    lengths[player] = longest_road_length(lane, player);
    best = max(best, lengths[player]);
    player += 1u;
  }
  var leader_count = 0u;
  var sole_leader = 0u;
  let old_holder = state_get(STATE_LONGEST_HOLDER, lane);
  var old_holder_is_leader = false;
  player = 0u;
  loop {
    if (player >= players) { break; }
    if (lengths[player] == best && best >= 5u) {
      leader_count += 1u;
      sole_leader = player;
      if (old_holder == player + 1u) { old_holder_is_leader = true; }
    }
    player += 1u;
  }
  var next_holder = 0u;
  if (old_holder != 0u && old_holder_is_leader) { next_holder = old_holder; }
  else if (leader_count == 1u) { next_holder = sole_leader + 1u; }
  if (next_holder == old_holder) { return; }
  if (old_holder != 0u) {
    let old_player = old_holder - 1u;
    player_set(lane, old_player, PLAYER_HAS_LONGEST, 0u);
    player_set(lane, old_player, PLAYER_PUBLIC_VP, player_get(lane, old_player, PLAYER_PUBLIC_VP) - 2u);
  }
  if (next_holder != 0u) {
    let new_player = next_holder - 1u;
    player_set(lane, new_player, PLAYER_HAS_LONGEST, 1u);
    player_set(lane, new_player, PLAYER_PUBLIC_VP, player_get(lane, new_player, PLAYER_PUBLIC_VP) + 2u);
  }
  state_set(STATE_LONGEST_HOLDER, lane, next_holder);
}
fn place_road_piece(lane: u32, edge: u32) -> bool {
  if (edge >= EDGE_COUNT) { return false; }
  let player = state_get(STATE_CURRENT_PLAYER, lane);
  let left = player_get(lane, player, PLAYER_ROADS_LEFT);
  if (left == 0u) { return false; }
  player_set(lane, player, PLAYER_ROADS_LEFT, left - 1u);
  state_set(STATE_ROADS + edge, lane, player + 1u);
  return true;
}
fn place_settlement_piece(lane: u32, vertex: u32) -> bool {
  if (vertex >= VERTEX_COUNT) { return false; }
  let player = state_get(STATE_CURRENT_PLAYER, lane);
  let left = player_get(lane, player, PLAYER_SETTLEMENTS_LEFT);
  if (left == 0u) { return false; }
  player_set(lane, player, PLAYER_SETTLEMENTS_LEFT, left - 1u);
  player_set(lane, player, PLAYER_PUBLIC_VP, player_get(lane, player, PLAYER_PUBLIC_VP) + 1u);
  state_set(STATE_BUILDINGS + vertex, lane, player + 1u);
  return true;
}
fn finish_if_won(lane: u32) {
  let player = state_get(STATE_CURRENT_PLAYER, lane);
  let vp = player_get(lane, player, PLAYER_PUBLIC_VP) + player_get(lane, player, PLAYER_DEVELOPMENT + 1u);
  if (vp >= state_get(STATE_VICTORY_TARGET, lane)) {
    state_set(STATE_PHASE, lane, PHASE_FINISHED);
    state_set(STATE_PHASE_ARG, lane, 0u);
  }
}

fn consume_development(lane: u32, card: u32) -> bool {
  let player = state_get(STATE_CURRENT_PLAYER, lane);
  if (player_get(lane, player, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN) != 0u) { return false; }
  let held = player_get(lane, player, PLAYER_DEVELOPMENT + card);
  let bought = player_get(lane, player, PLAYER_BOUGHT_DEVELOPMENT + card);
  if (held == 0u || held <= bought) { return false; }
  player_set(lane, player, PLAYER_DEVELOPMENT + card, held - 1u);
  state_set(STATE_PLAYED_DEVELOPMENT + card, lane, state_get(STATE_PLAYED_DEVELOPMENT + card, lane) + 1u);
  player_set(lane, player, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN, 1u);
  return true;
}
fn development_playable(lane: u32, player: u32, card: u32) -> bool {
  if (player_get(lane, player, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN) != 0u) { return false; }
  let held = player_get(lane, player, PLAYER_DEVELOPMENT + card);
  let bought = player_get(lane, player, PLAYER_BOUGHT_DEVELOPMENT + card);
  return held > bought;
}
fn update_largest_army(lane: u32) {
  let players = state_get(STATE_NUM_PLAYERS, lane);
  var best = 0u;
  var player = 0u;
  loop {
    if (player >= players) { break; }
    best = max(best, player_get(lane, player, PLAYER_PLAYED_KNIGHTS));
    player += 1u;
  }
  var leader_count = 0u;
  var sole_leader = 0u;
  let old_holder = state_get(STATE_LARGEST_HOLDER, lane);
  var old_holder_is_leader = false;
  player = 0u;
  loop {
    if (player >= players) { break; }
    let played = player_get(lane, player, PLAYER_PLAYED_KNIGHTS);
    if (played == best && best >= 3u) {
      leader_count += 1u;
      sole_leader = player;
      if (old_holder == player + 1u) { old_holder_is_leader = true; }
    }
    player += 1u;
  }
  var next_holder = 0u;
  if (old_holder != 0u && old_holder_is_leader) { next_holder = old_holder; }
  else if (leader_count == 1u) { next_holder = sole_leader + 1u; }
  if (next_holder == old_holder) { return; }
  if (old_holder != 0u) {
    let old_player = old_holder - 1u;
    player_set(lane, old_player, PLAYER_HAS_LARGEST, 0u);
    player_set(lane, old_player, PLAYER_PUBLIC_VP, player_get(lane, old_player, PLAYER_PUBLIC_VP) - 2u);
  }
  if (next_holder != 0u) {
    let new_player = next_holder - 1u;
    player_set(lane, new_player, PLAYER_HAS_LARGEST, 1u);
    player_set(lane, new_player, PLAYER_PUBLIC_VP, player_get(lane, new_player, PLAYER_PUBLIC_VP) + 2u);
  }
  state_set(STATE_LARGEST_HOLDER, lane, next_holder);
}
fn restore_robber_return_phase(lane: u32) {
  state_set(STATE_PHASE, lane, state_get(STATE_ROBBER_RETURN_PHASE, lane));
  state_set(STATE_PHASE_ARG, lane, state_get(STATE_ROBBER_RETURN_ARG, lane));
}
fn produce_roll(lane: u32, roll: u32) {
  var demand: array<u32, 20>;
  var vertex = 0u;
  loop {
    if (vertex >= VERTEX_COUNT) { break; }
    let building = state_get(STATE_BUILDINGS + vertex, lane);
    let player = building_player(building);
    if (player != 0xffffffffu) {
      let multiplier = building_multiplier(building);
      let count = topo_vertex_hex_count(vertex);
      var slot = 0u;
      loop {
        if (slot >= count) { break; }
        let hex = topo_vertex_hex(vertex, slot);
        if (hex != state_get(STATE_ROBBER_HEX, lane) && state_get(STATE_HEX_NUMBERS + hex, lane) == roll) {
          let encoded = state_get(STATE_HEX_RESOURCES + hex, lane);
          if (encoded > 0u) { demand[player * 5u + encoded - 1u] += multiplier; }
        }
        slot += 1u;
      }
    }
    vertex += 1u;
  }
  let players = state_get(STATE_NUM_PLAYERS, lane);
  var res = 0u;
  loop {
    if (res >= 5u) { break; }
    var total = 0u;
    var player = 0u;
    loop {
      if (player >= players) { break; }
      total += demand[player * 5u + res];
      player += 1u;
    }
    let bank = state_get(STATE_BANK + res, lane);
    if (total <= bank) {
      state_set(STATE_BANK + res, lane, bank - total);
      player = 0u;
      loop {
        if (player >= players) { break; }
        let gain = demand[player * 5u + res];
        if (gain > 0u) { player_set(lane, player, PLAYER_RESOURCES + res, player_get(lane, player, PLAYER_RESOURCES + res) + gain); }
        player += 1u;
      }
    }
    res += 1u;
  }
}

fn robber_hex_allowed(lane: u32, hex: u32) -> bool {
  if (hex >= HEX_COUNT || hex == state_get(STATE_ROBBER_HEX, lane)) { return false; }
  if (state_get(STATE_FRIENDLY_ROBBER, lane) == 0u) { return true; }
  var vertex = 0u;
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  loop {
    if (vertex >= VERTEX_COUNT) { break; }
    if (building_player(state_get(STATE_BUILDINGS + vertex, lane)) == current) {
      let count = topo_vertex_hex_count(vertex);
      var slot = 0u;
      loop {
        if (slot >= count) { break; }
        if (topo_vertex_hex(vertex, slot) == hex) { return false; }
        slot += 1u;
      }
    }
    vertex += 1u;
  }
  return true;
}
fn robber_victim_mask(lane: u32, hex: u32) -> u32 {
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  let players = state_get(STATE_NUM_PLAYERS, lane);
  var mask = 0u;
  var vertex = 0u;
  loop {
    if (vertex >= VERTEX_COUNT) { break; }
    let player = building_player(state_get(STATE_BUILDINGS + vertex, lane));
    if (player != 0xffffffffu && player != current && player < players && resource_total(lane, player) > 0u) {
      let count = topo_vertex_hex_count(vertex);
      var slot = 0u;
      loop {
        if (slot >= count) { break; }
        if (topo_vertex_hex(vertex, slot) == hex) { mask |= 1u << player; break; }
        slot += 1u;
      }
    }
    vertex += 1u;
  }
  return mask;
}
fn robber_policy_score(lane: u32, hex: u32, victim_code: u32) -> u32 {
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  var opponent_pressure = 0u;
  var self_penalty = 0u;
  var vertex = 0u;
  loop {
    if (vertex >= VERTEX_COUNT) { break; }
    let building = state_get(STATE_BUILDINGS + vertex, lane);
    let owner = building_player(building);
    if (owner != 0xffffffffu) {
      let count = topo_vertex_hex_count(vertex);
      var touches = false;
      var slot = 0u;
      loop {
        if (slot >= count) { break; }
        if (topo_vertex_hex(vertex, slot) == hex) { touches = true; break; }
        slot += 1u;
      }
      if (touches) {
        let production = pips_for_number(state_get(STATE_HEX_NUMBERS + hex, lane)) * building_multiplier(building);
        if (owner == current) { self_penalty += production * 45u; }
        else { opponent_pressure += production * (50u + player_get(lane, owner, PLAYER_PUBLIC_VP) * 4u); }
      }
    }
    vertex += 1u;
  }
  var score = 100u + opponent_pressure;
  if (victim_code != 0u) {
    let victim = victim_code - 1u;
    score += resource_total(lane, victim) * 16u + player_get(lane, victim, PLAYER_PUBLIC_VP) * 28u;
  }
  return select(10u, score - self_penalty, score > self_penalty + 10u);
}
fn choose_robber(lane: u32, rng: ptr<function, U64>) -> RobberChoice {
  var total = 0u;
  var selected = RobberChoice(0u, 0u, 0u);
  var hex = 0u;
  loop {
    if (hex >= HEX_COUNT) { break; }
    if (robber_hex_allowed(lane, hex)) {
      let mask = robber_victim_mask(lane, hex);
      if (mask == 0u) {
        let weight = robber_policy_score(lane, hex, 0u);
        if (weighted_select(rng, &total, weight)) { selected = RobberChoice(1u, hex, 0u); }
      } else {
        var victim = 0u;
        let players = state_get(STATE_NUM_PLAYERS, lane);
        loop {
          if (victim >= players) { break; }
          if ((mask & (1u << victim)) != 0u) {
            let weight = robber_policy_score(lane, hex, victim + 1u);
            if (weighted_select(rng, &total, weight)) { selected = RobberChoice(1u, hex, victim + 1u); }
          }
          victim += 1u;
        }
      }
    }
    hex += 1u;
  }
  return selected;
}

fn generate_rollout_action(lane: u32) {
  clear_action(lane);
  let phase = state_get(STATE_PHASE, lane);
  let chance = phase == PHASE_ROLL_CHANCE || phase == PHASE_DEVELOPMENT_CHANCE || phase == PHASE_RESOLVE_STEAL;
  var rng = load_rng(lane, chance);
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  let players = state_get(STATE_NUM_PLAYERS, lane);
  var seen = false;

  if (phase == PHASE_PRE_ROLL) {
    write_action(lane, ACTION_ROLL, 0u, 0u, 0u);
    seen = true;
  } else if (phase == PHASE_ROLL_CHANCE) {
    write_action(lane, ACTION_RESOLVE_ROLL, rng_range(&rng, 6u) + rng_range(&rng, 6u) + 2u, 0u, 0u);
    seen = true;
  } else if (phase == PHASE_DISCARD) {
    let player = state_get(STATE_DISCARD_CURSOR, lane);
    let required = state_get(STATE_DISCARD_REMAINING + player, lane);
    var remaining: array<u32, 5>;
    var discard_cards: array<u32, 5>;
    var res = 0u;
    loop {
      if (res >= 5u) { break; }
      remaining[res] = player_get(lane, player, PLAYER_RESOURCES + res);
      res += 1u;
    }
    var card = 0u;
    loop {
      if (card >= required) { break; }
      var total = 0u;
      res = 0u;
      loop { if (res >= 5u) { break; } total += remaining[res]; res += 1u; }
      if (total == 0u) { break; }
      var pick = rng_range(&rng, total);
      res = 0u;
      loop {
        if (res >= 5u) { break; }
        if (pick < remaining[res]) { remaining[res] -= 1u; discard_cards[res] += 1u; break; }
        pick -= remaining[res];
        res += 1u;
      }
      card += 1u;
    }
    write_action(lane, ACTION_DISCARD, discard_cards[0], discard_cards[1], discard_cards[2]);
    action_set(ACTION_ARG0 + 3u, lane, discard_cards[3]);
    action_set(ACTION_ARG0 + 4u, lane, discard_cards[4]);
    seen = true;
  } else if (phase == PHASE_MOVE_ROBBER) {
    let choice = choose_robber(lane, &rng);
    if (choice.found != 0u) { write_action(lane, ACTION_MOVE_ROBBER, choice.hex, choice.victim_code, 0u); seen = true; }
  } else if (phase == PHASE_RESOLVE_STEAL) {
    let victim = state_get(STATE_PHASE_ARG, lane);
    let total = resource_total(lane, victim);
    if (total > 0u) {
      var pick = rng_range(&rng, total);
      var res = 0u;
      loop {
        if (res >= 5u) { break; }
        let held = player_get(lane, victim, PLAYER_RESOURCES + res);
        if (pick < held) { write_action(lane, ACTION_RESOLVE_STEAL, victim, res, 0u); seen = true; break; }
        pick -= held;
        res += 1u;
      }
    }
  } else if (phase == PHASE_MAIN) {
    var family_weight = 0u;
    let actor_vp = player_get(lane, current, PLAYER_PUBLIC_VP) + player_get(lane, current, PLAYER_DEVELOPMENT + 1u);
    let victory_target = state_get(STATE_VICTORY_TARGET, lane);
    if (weighted_select(&rng, &family_weight, profile_scaled_weight(lane, current, 0u, 120u))) {
      write_action(lane, ACTION_END_TURN, 0u, 0u, 0u);
    }
    if (player_get(lane, current, PLAYER_ROADS_LEFT) > 0u && has_cost(lane, current, 0u)) {
      var candidate_weight = 0u;
      var selected_edge = 0xffffffffu;
      var edge = 0u;
      loop {
        if (edge >= EDGE_COUNT) { break; }
        if (can_build_road(lane, edge, 0xffffffffu)) {
          let weight = road_policy_score(lane, edge);
          if (weighted_select(&rng, &candidate_weight, weight)) { selected_edge = edge; }
        }
        edge += 1u;
      }
      if (selected_edge != 0xffffffffu && weighted_select(&rng, &family_weight, profile_scaled_weight(lane, current, 1u, 900u))) {
        write_action(lane, ACTION_BUILD_ROAD, selected_edge, 0u, 0u);
      }
    }
    if (player_get(lane, current, PLAYER_SETTLEMENTS_LEFT) > 0u && has_cost(lane, current, 1u)) {
      var candidate_weight = 0u;
      var selected_vertex = 0xffffffffu;
      var vertex = 0u;
      loop {
        if (vertex >= VERTEX_COUNT) { break; }
        if (can_place_settlement(lane, vertex)) {
          let weight = vertex_policy_score(lane, vertex);
          if (weighted_select(&rng, &candidate_weight, weight)) { selected_vertex = vertex; }
        }
        vertex += 1u;
      }
      if (selected_vertex != 0xffffffffu) {
        let base = select(3200u, 24000u, actor_vp + 1u >= victory_target);
        if (weighted_select(&rng, &family_weight, profile_scaled_weight(lane, current, 1u, base))) {
          write_action(lane, ACTION_BUILD_SETTLEMENT, selected_vertex, 0u, 0u);
        }
      }
    }
    if (player_get(lane, current, PLAYER_CITIES_LEFT) > 0u && has_cost(lane, current, 2u)) {
      var candidate_weight = 0u;
      var selected_vertex = 0xffffffffu;
      var vertex = 0u;
      loop {
        if (vertex >= VERTEX_COUNT) { break; }
        if (state_get(STATE_BUILDINGS + vertex, lane) == current + 1u) {
          let weight = vertex_policy_score(lane, vertex) + 200u;
          if (weighted_select(&rng, &candidate_weight, weight)) { selected_vertex = vertex; }
        }
        vertex += 1u;
      }
      if (selected_vertex != 0xffffffffu) {
        let base = select(4200u, 26000u, actor_vp + 1u >= victory_target);
        if (weighted_select(&rng, &family_weight, profile_scaled_weight(lane, current, 2u, base))) {
          write_action(lane, ACTION_BUILD_CITY, selected_vertex, 0u, 0u);
        }
      }
    }
    var deck_total = 0u;
    var dev = 0u;
    loop { if (dev >= 5u) { break; } deck_total += state_get(STATE_DEVELOPMENT_DECK + dev, lane); dev += 1u; }
    if (deck_total > 0u && has_cost(lane, current, 3u) && weighted_select(&rng, &family_weight, profile_scaled_weight(lane, current, 2u, 900u))) {
      write_action(lane, ACTION_BUY_DEVELOPMENT, 0u, 0u, 0u);
    }
    if (state_get(STATE_BANK_PUBLIC, lane) != 0u) {
      var maritime_weight = 0u;
      var maritime_give = 0xffffffffu;
      var maritime_receive = 0xffffffffu;
      var maritime_ratio = 0u;
      var give = 0u;
      loop {
        if (give >= 5u) { break; }
        let ratio = trade_ratio(lane, current, give);
        if (player_get(lane, current, PLAYER_RESOURCES + give) >= ratio) {
          var receive = 0u;
          loop {
            if (receive >= 5u) { break; }
            if (give != receive && state_get(STATE_BANK + receive, lane) > 0u) {
              let weight = maritime_policy_score(lane, current, give, receive, ratio);
              if (weighted_select(&rng, &maritime_weight, weight)) { maritime_give = give; maritime_receive = receive; maritime_ratio = ratio; }
            }
            receive += 1u;
          }
        }
        give += 1u;
      }
      if (maritime_give != 0xffffffffu && weighted_select(&rng, &family_weight, profile_scaled_weight(lane, current, 0u, 700u))) {
        write_action(lane, ACTION_MARITIME_TRADE, maritime_give, maritime_receive, maritime_ratio);
      }
    }
    seen = family_weight > 0u;
  } else if (phase == PHASE_DEVELOPMENT_CHANCE) {
    var total = 0u;
    var card = 0u;
    loop { if (card >= 5u) { break; } total += state_get(STATE_DEVELOPMENT_DECK + card, lane); card += 1u; }
    if (total > 0u) {
      var pick = rng_range(&rng, total);
      card = 0u;
      loop {
        if (card >= 5u) { break; }
        let held = state_get(STATE_DEVELOPMENT_DECK + card, lane);
        if (pick < held) { write_action(lane, ACTION_RESOLVE_DEVELOPMENT, card, 0u, 0u); seen = true; break; }
        pick -= held;
        card += 1u;
      }
    }
  } else if (phase == PHASE_FINISHED) {
    write_action(lane, 255u, 0u, 0u, 0u);
    seen = true;
  }
  if (!seen) { write_action(lane, 254u, 0u, 0u, 0u); }
  store_rng(lane, chance, rng);
}

fn apply_transition(lane: u32) {
  if (status_get(lane) != STATUS_OK) { return; }
  let tag = action_get(ACTION_TAG, lane);
  let phase = state_get(STATE_PHASE, lane);
  let current = state_get(STATE_CURRENT_PLAYER, lane);
  let players = state_get(STATE_NUM_PLAYERS, lane);
  if (tag == 255u && phase == PHASE_FINISHED) { return; }

  if (tag == ACTION_ROLL) {
    if (phase != PHASE_PRE_ROLL) { status_set(lane, STATUS_INVALID_PHASE); return; }
    state_set(STATE_PHASE, lane, PHASE_ROLL_CHANCE);
    state_set(STATE_PHASE_ARG, lane, 0u);
    return;
  }
  if (tag == ACTION_RESOLVE_ROLL) {
    if (phase != PHASE_ROLL_CHANCE) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let roll = action_get(ACTION_ARG0, lane);
    if (roll < 2u || roll > 12u) { status_set(lane, STATUS_INVALID_ACTION); return; }
    state_set(STATE_LAST_ROLL, lane, roll);
    if (roll == 7u) {
      var player = 0u;
      loop { if (player >= 4u) { break; } state_set(STATE_DISCARD_REMAINING + player, lane, 0u); player += 1u; }
      let limit = state_get(STATE_DISCARD_LIMIT, lane);
      var next_discarder = 0xffffffffu;
      player = 0u;
      loop {
        if (player >= players) { break; }
        let total = resource_total(lane, player);
        if (total > limit) { state_set(STATE_DISCARD_REMAINING + player, lane, total / 2u); if (next_discarder == 0xffffffffu) { next_discarder = player; } }
        player += 1u;
      }
      if (next_discarder != 0xffffffffu) {
        state_set(STATE_DISCARD_CURSOR, lane, next_discarder);
        state_set(STATE_PHASE, lane, PHASE_DISCARD);
      } else {
        state_set(STATE_ROBBER_RETURN_PHASE, lane, PHASE_MAIN);
        state_set(STATE_ROBBER_RETURN_ARG, lane, 0u);
        state_set(STATE_PHASE, lane, PHASE_MOVE_ROBBER);
      }
      state_set(STATE_PHASE_ARG, lane, 0u);
    } else {
      produce_roll(lane, roll);
      state_set(STATE_PHASE, lane, PHASE_MAIN);
      state_set(STATE_PHASE_ARG, lane, 0u);
    }
    return;
  }
  if (tag == ACTION_DISCARD) {
    if (phase != PHASE_DISCARD) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let player = state_get(STATE_DISCARD_CURSOR, lane);
    let required = state_get(STATE_DISCARD_REMAINING + player, lane);
    var total = 0u;
    var res = 0u;
    loop {
      if (res >= 5u) { break; }
      let cards = action_get(ACTION_ARG0 + res, lane);
      if (cards > player_get(lane, player, PLAYER_RESOURCES + res)) { status_set(lane, STATUS_INVALID_ACTION); return; }
      total += cards;
      res += 1u;
    }
    if (total != required) { status_set(lane, STATUS_INVALID_ACTION); return; }
    res = 0u;
    loop {
      if (res >= 5u) { break; }
      let cards = action_get(ACTION_ARG0 + res, lane);
      player_set(lane, player, PLAYER_RESOURCES + res, player_get(lane, player, PLAYER_RESOURCES + res) - cards);
      state_set(STATE_BANK + res, lane, state_get(STATE_BANK + res, lane) + cards);
      res += 1u;
    }
    state_set(STATE_DISCARD_REMAINING + player, lane, 0u);
    var next = 0xffffffffu;
    var candidate = player + 1u;
    loop {
      if (candidate >= players) { break; }
      if (state_get(STATE_DISCARD_REMAINING + candidate, lane) > 0u) { next = candidate; break; }
      candidate += 1u;
    }
    if (next != 0xffffffffu) { state_set(STATE_DISCARD_CURSOR, lane, next); }
    else {
      state_set(STATE_ROBBER_RETURN_PHASE, lane, PHASE_MAIN);
      state_set(STATE_ROBBER_RETURN_ARG, lane, 0u);
      state_set(STATE_PHASE, lane, PHASE_MOVE_ROBBER);
      state_set(STATE_PHASE_ARG, lane, 0u);
    }
    return;
  }
  if (tag == ACTION_MOVE_ROBBER) {
    if (phase != PHASE_MOVE_ROBBER) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let hex = action_get(ACTION_ARG0, lane);
    let victim_code = action_get(ACTION_ARG0 + 1u, lane);
    if (hex >= HEX_COUNT || hex == state_get(STATE_ROBBER_HEX, lane)) { status_set(lane, STATUS_INVALID_ACTION); return; }
    state_set(STATE_ROBBER_HEX, lane, hex);
    if (victim_code == 0u) { restore_robber_return_phase(lane); }
    else {
      let victim = victim_code - 1u;
      if (victim >= players || victim == current) { status_set(lane, STATUS_INVALID_ACTION); return; }
      state_set(STATE_PHASE, lane, PHASE_RESOLVE_STEAL);
      state_set(STATE_PHASE_ARG, lane, victim);
    }
    return;
  }
  if (tag == ACTION_RESOLVE_STEAL) {
    let victim = action_get(ACTION_ARG0, lane);
    let res = action_get(ACTION_ARG0 + 1u, lane);
    if (phase != PHASE_RESOLVE_STEAL || state_get(STATE_PHASE_ARG, lane) != victim) { status_set(lane, STATUS_INVALID_PHASE); return; }
    if (victim >= players || res >= 5u || player_get(lane, victim, PLAYER_RESOURCES + res) == 0u) { status_set(lane, STATUS_INVALID_ACTION); return; }
    player_set(lane, victim, PLAYER_RESOURCES + res, player_get(lane, victim, PLAYER_RESOURCES + res) - 1u);
    player_set(lane, current, PLAYER_RESOURCES + res, player_get(lane, current, PLAYER_RESOURCES + res) + 1u);
    restore_robber_return_phase(lane);
    return;
  }
  if (tag == ACTION_BUILD_ROAD) {
    if (phase != PHASE_MAIN) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let edge = action_get(ACTION_ARG0, lane);
    if (!pay_cost(lane, 0u) || !place_road_piece(lane, edge)) { status_set(lane, STATUS_INVALID_ACTION); return; }
    update_longest_road(lane);
    finish_if_won(lane);
    return;
  }
  if (tag == ACTION_BUILD_SETTLEMENT) {
    if (phase != PHASE_MAIN) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let vertex = action_get(ACTION_ARG0, lane);
    if (!pay_cost(lane, 1u) || !place_settlement_piece(lane, vertex)) { status_set(lane, STATUS_INVALID_ACTION); return; }
    update_longest_road(lane);
    finish_if_won(lane);
    return;
  }
  if (tag == ACTION_BUILD_CITY) {
    if (phase != PHASE_MAIN) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let vertex = action_get(ACTION_ARG0, lane);
    if (vertex >= VERTEX_COUNT || state_get(STATE_BUILDINGS + vertex, lane) != current + 1u || player_get(lane, current, PLAYER_CITIES_LEFT) == 0u || !pay_cost(lane, 2u)) { status_set(lane, STATUS_INVALID_ACTION); return; }
    player_set(lane, current, PLAYER_CITIES_LEFT, player_get(lane, current, PLAYER_CITIES_LEFT) - 1u);
    player_set(lane, current, PLAYER_SETTLEMENTS_LEFT, player_get(lane, current, PLAYER_SETTLEMENTS_LEFT) + 1u);
    player_set(lane, current, PLAYER_PUBLIC_VP, player_get(lane, current, PLAYER_PUBLIC_VP) + 1u);
    state_set(STATE_BUILDINGS + vertex, lane, current + 5u);
    finish_if_won(lane);
    return;
  }
  if (tag == ACTION_BUY_DEVELOPMENT) {
    if (phase != PHASE_MAIN) { status_set(lane, STATUS_INVALID_PHASE); return; }
    var total = 0u;
    var card = 0u;
    loop { if (card >= 5u) { break; } total += state_get(STATE_DEVELOPMENT_DECK + card, lane); card += 1u; }
    if (total == 0u || !pay_cost(lane, 3u)) { status_set(lane, STATUS_INVALID_ACTION); return; }
    state_set(STATE_PHASE, lane, PHASE_DEVELOPMENT_CHANCE);
    state_set(STATE_PHASE_ARG, lane, 0u);
    return;
  }
  if (tag == ACTION_RESOLVE_DEVELOPMENT) {
    if (phase != PHASE_DEVELOPMENT_CHANCE) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let card = action_get(ACTION_ARG0, lane);
    if (card >= 5u || state_get(STATE_DEVELOPMENT_DECK + card, lane) == 0u) { status_set(lane, STATUS_INVALID_ACTION); return; }
    state_set(STATE_DEVELOPMENT_DECK + card, lane, state_get(STATE_DEVELOPMENT_DECK + card, lane) - 1u);
    player_set(lane, current, PLAYER_DEVELOPMENT + card, player_get(lane, current, PLAYER_DEVELOPMENT + card) + 1u);
    player_set(lane, current, PLAYER_BOUGHT_DEVELOPMENT + card, player_get(lane, current, PLAYER_BOUGHT_DEVELOPMENT + card) + 1u);
    state_set(STATE_PHASE, lane, PHASE_MAIN);
    state_set(STATE_PHASE_ARG, lane, 0u);
    finish_if_won(lane);
    return;
  }
  if (tag == ACTION_MARITIME_TRADE) {
    if (phase != PHASE_MAIN) { status_set(lane, STATUS_INVALID_PHASE); return; }
    let give = action_get(ACTION_ARG0, lane);
    let receive = action_get(ACTION_ARG0 + 1u, lane);
    let ratio = action_get(ACTION_ARG0 + 2u, lane);
    if (give >= 5u || receive >= 5u || give == receive) { status_set(lane, STATUS_INVALID_ACTION); return; }
    let actual = trade_ratio(lane, current, give);
    let held = player_get(lane, current, PLAYER_RESOURCES + give);
    let bank_receive = state_get(STATE_BANK + receive, lane);
    if (ratio != actual || held < ratio || bank_receive == 0u) { status_set(lane, STATUS_INVALID_ACTION); return; }
    player_set(lane, current, PLAYER_RESOURCES + give, held - ratio);
    state_set(STATE_BANK + give, lane, state_get(STATE_BANK + give, lane) + ratio);
    state_set(STATE_BANK + receive, lane, bank_receive - 1u);
    player_set(lane, current, PLAYER_RESOURCES + receive, player_get(lane, current, PLAYER_RESOURCES + receive) + 1u);
    return;
  }
  if (tag == ACTION_END_TURN) {
    if (phase != PHASE_MAIN) { status_set(lane, STATUS_INVALID_PHASE); return; }
    var card = 0u;
    loop { if (card >= 5u) { break; } player_set(lane, current, PLAYER_BOUGHT_DEVELOPMENT + card, 0u); card += 1u; }
    player_set(lane, current, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN, 0u);
    state_set(STATE_DOMESTIC_TRADE_USED, lane, 0u);
    state_set(STATE_DOMESTIC_TRADE_COUNT, lane, 0u);
    state_set(STATE_TRADE_NEGOTIATION_ROUND, lane, 0u);
    state_set(STATE_CURRENT_PLAYER, lane, (current + 1u) % players);
    state_set(STATE_TURN, lane, state_get(STATE_TURN, lane) + 1u);
    state_set(STATE_LAST_ROLL, lane, 0u);
    state_set(STATE_PHASE, lane, PHASE_PRE_ROLL);
    state_set(STATE_PHASE_ARG, lane, 0u);
    finish_if_won(lane);
    return;
  }
  status_set(lane, STATUS_UNSUPPORTED_ACTION);
}

fn base_actor(base: u32) -> u32 {
  let phase = base_get(STATE_PHASE, base);
  if (phase == PHASE_DISCARD) { return base_get(STATE_DISCARD_CURSOR, base); }
  return base_get(STATE_CURRENT_PLAYER, base);
}

@compute @workgroup_size(1)
fn rng_probe(@builtin(global_invocation_id) gid: vec3<u32>) {
  if (gid.x != 0u) { return; }
  let seed = U64(params.seed_lo, params.seed_hi);
  let mixed = mix_stream_seed(seed, U64(0u, 0u), U64(0x299f31d0u, 0xa4093822u));
  var state = mixed;
  let next = splitmix64_next(&state);
  atomicStore(&stats[0], mixed.lo);
  atomicStore(&stats[1], mixed.hi);
  atomicStore(&stats[2], next.lo);
  atomicStore(&stats[3], next.hi);
  atomicStore(&stats[4], u64_mod_u32(next, 6u));
}

@compute @workgroup_size(128)
fn expand_root_rollouts(@builtin(global_invocation_id) gid: vec3<u32>) {
  let lane = gid.x;
  if (lane >= params.lane_count || params.chunk_rollouts == 0u) { return; }
  let root = lane / params.chunk_rollouts;
  if (root >= params.root_count) { return; }
  let base = root_base_index(root);
  var field = 0u;
  loop {
    if (field >= STATE_WORDS) { break; }
    state_set(field, lane, base_get(field, base));
    field += 1u;
  }
  field = 0u;
  loop {
    if (field >= ACTION_WORDS) { break; }
    action_set(field, lane, root_action_get(field, root));
    field += 1u;
  }
  status_set(lane, STATUS_OK);
  let rollout = params.rollout_offset + (lane % params.chunk_rollouts);
  let seed = U64(params.seed_lo, params.seed_hi);
  let keyed = u64_xor(seed, U64(root, 0u));
  let rng = mix_stream_seed(keyed, U64(rollout, 0u), U64(0x299f31d0u, 0xa4093822u));
  let chance_rng = mix_stream_seed(keyed, U64(rollout, 0u), U64(0xec4e6c89u, 0x082efa98u));
  lane_data[rng_lo_base() + lane] = rng.lo;
  lane_data[rng_hi_base() + lane] = rng.hi;
  lane_data[chance_lo_base() + lane] = chance_rng.lo;
  lane_data[chance_hi_base() + lane] = chance_rng.hi;
  apply_transition(lane);
}

@compute @workgroup_size(128)
fn run_rollout_steps(@builtin(global_invocation_id) gid: vec3<u32>) {
  let lane = gid.x;
  if (lane >= params.lane_count) { return; }
  var step = 0u;
  loop {
    if (step >= params.step_count || status_get(lane) != STATUS_OK || state_get(STATE_PHASE, lane) == PHASE_FINISHED) { break; }
    generate_rollout_action(lane);
    apply_transition(lane);
    step += 1u;
  }
}

@compute @workgroup_size(128)
fn reduce_root_rollouts(@builtin(global_invocation_id) gid: vec3<u32>) {
  let lane = gid.x;
  if (lane >= params.lane_count || params.chunk_rollouts == 0u) { return; }
  let root = lane / params.chunk_rollouts;
  if (root >= params.root_count) { return; }
  atomicAdd(&stats[0u * params.root_count + root], 1u);
  if (status_get(lane) != STATUS_OK) {
    atomicAdd(&stats[1u * params.root_count + root], 1u);
    return;
  }
  let base = root_base_index(root);
  let actor = base_actor(base);
  let players = state_get(STATE_NUM_PLAYERS, lane);
  let victory_target = state_get(STATE_VICTORY_TARGET, lane);
  let terminal = state_get(STATE_PHASE, lane) == PHASE_FINISHED;
  var actor_vp = 0u;
  var best_opponent = 0u;
  var winner = 0xffffffffu;
  var player = 0u;
  loop {
    if (player >= players) { break; }
    let vp = player_get(lane, player, PLAYER_PUBLIC_VP) + player_get(lane, player, PLAYER_DEVELOPMENT + 1u);
    if (player == actor) { actor_vp = vp; } else { best_opponent = max(best_opponent, vp); }
    if (terminal && winner == 0xffffffffu && vp >= victory_target) { winner = player; }
    player += 1u;
  }
  if (terminal) {
    atomicAdd(&stats[2u * params.root_count + root], 1u);
    if (winner == actor) { atomicAdd(&stats[3u * params.root_count + root], 1u); }
  }
  atomicAdd(&stats[4u * params.root_count + root], state_get(STATE_TURN, lane));
  atomicAdd(&stats[5u * params.root_count + root], actor_vp);
  atomicAdd(&stats[6u * params.root_count + root], best_opponent);
  let margin = i32(actor_vp) - i32(best_opponent);
  atomicAdd(&stats[7u * params.root_count + root], u32(margin * margin));
  atomicAdd(&stats[8u * params.root_count + root], actor_vp * actor_vp);
  atomicAdd(&stats[9u * params.root_count + root], best_opponent * best_opponent);
}
