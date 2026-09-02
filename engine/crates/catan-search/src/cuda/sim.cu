// First GPU-resident Catan transition kernel.
//
// States use a field-major (SoA) layout: field * stride + lane. The buffer is
// mutated in place and remains resident across successive transition launches.
// This first lane intentionally supports the canonical no-player-trades rules
// subset needed to validate persistent GPU state before GPU legality/search are
// layered on top.
typedef unsigned int uint32_t;
typedef unsigned long long uint64_t;

#define MAX_PLAYERS 4u
#define HEX_COUNT 19u
#define VERTEX_COUNT 54u
#define EDGE_COUNT 72u
#define MAX_VERTEX_ADJACENCY 3u

#define STATE_NUM_PLAYERS 0u
#define STATE_PHASE 1u
#define STATE_PHASE_ARG 2u
#define STATE_CURRENT_PLAYER 3u
#define STATE_ROBBER_HEX 4u
#define STATE_VICTORY_TARGET 5u
#define STATE_DISCARD_LIMIT 6u
#define STATE_BANK_PUBLIC 7u
#define STATE_LONGEST_HOLDER 8u
#define STATE_LARGEST_HOLDER 9u
#define STATE_TURN 10u
#define STATE_LAST_ROLL 11u
#define STATE_FRIENDLY_ROBBER 12u
#define STATE_SETUP_STEP 13u
#define STATE_DISCARD_CURSOR 14u
#define STATE_ROBBER_RETURN_PHASE 15u
#define STATE_ROBBER_RETURN_ARG 16u
#define STATE_FREE_ROADS 17u
#define STATE_DOMESTIC_TRADE_USED 18u
#define STATE_DOMESTIC_TRADE_COUNT 19u
#define STATE_PLAYER_TRADES_ENABLED 20u
#define STATE_TRADE_CURSOR 21u
#define STATE_TRADE_NEGOTIATION_ROUND 22u
#define STATE_BANK 23u
#define STATE_DEVELOPMENT_DECK 28u
#define STATE_PLAYED_DEVELOPMENT 33u
#define STATE_DISCARD_REMAINING 38u
#define STATE_HEX_RESOURCES 42u
#define STATE_HEX_NUMBERS 61u
#define STATE_PORTS 80u
#define STATE_BUILDINGS 134u
#define STATE_ROADS 188u
#define STATE_PLAYERS 260u
#define PLAYER_STRIDE 28u
#define STATE_WORDS 372u

#define PLAYER_RESOURCES 0u
#define PLAYER_DEVELOPMENT 5u
#define PLAYER_BOUGHT_DEVELOPMENT 10u
#define PLAYER_PUBLIC_VP 15u
#define PLAYER_PLAYED_KNIGHTS 16u
#define PLAYER_ROADS_LEFT 17u
#define PLAYER_SETTLEMENTS_LEFT 18u
#define PLAYER_CITIES_LEFT 19u
#define PLAYER_HAS_LONGEST 20u
#define PLAYER_HAS_LARGEST 21u
#define PLAYER_PLAYED_DEVELOPMENT_THIS_TURN 22u
#define PLAYER_POLICY_PROFILE 23u

#define ACTION_TAG 0u
#define ACTION_ARG0 1u

#define ACTION_PLACE_SETTLEMENT 0u
#define ACTION_PLACE_ROAD 1u
#define ACTION_ROLL 2u
#define ACTION_RESOLVE_ROLL 3u
#define ACTION_DISCARD 4u
#define ACTION_MOVE_ROBBER 5u
#define ACTION_RESOLVE_STEAL 6u
#define ACTION_BUILD_ROAD 7u
#define ACTION_BUILD_SETTLEMENT 8u
#define ACTION_BUILD_CITY 9u
#define ACTION_BUY_DEVELOPMENT 10u
#define ACTION_RESOLVE_DEVELOPMENT 11u
#define ACTION_PLAY_KNIGHT 12u
#define ACTION_PLAY_ROAD_BUILDING 13u
#define ACTION_PLAY_YEAR_OF_PLENTY 14u
#define ACTION_PLAY_MONOPOLY 15u
#define ACTION_MARITIME_TRADE 16u
#define ACTION_END_TURN 17u

#define PHASE_SETUP_SETTLEMENT 0u
#define PHASE_SETUP_ROAD 1u
#define PHASE_PRE_ROLL 2u
#define PHASE_ROLL_CHANCE 3u
#define PHASE_DISCARD 4u
#define PHASE_MOVE_ROBBER 5u
#define PHASE_RESOLVE_STEAL 6u
#define PHASE_MAIN 7u
#define PHASE_DEVELOPMENT_CHANCE 8u
#define PHASE_TRADE_RESPONSES 9u
#define PHASE_FINISHED 10u

#define STATUS_OK 0u
#define STATUS_UNSUPPORTED_ACTION 1u
#define STATUS_INVALID_PHASE 2u
#define STATUS_INVALID_ACTION 3u
#define STATUS_INVALID_STATE 4u

#define TOPO_VERTEX_HEX_COUNTS 0u
#define TOPO_VERTEX_HEXES 54u
#define TOPO_VERTEX_VERTEX_COUNTS 216u
#define TOPO_VERTEX_VERTICES 270u
#define TOPO_VERTEX_EDGE_COUNTS 432u
#define TOPO_VERTEX_EDGES 486u
#define TOPO_EDGE_VERTICES 648u

static const uint32_t ROAD_COST[5] = {1u, 1u, 0u, 0u, 0u};
static const uint32_t SETTLEMENT_COST[5] = {1u, 1u, 1u, 1u, 0u};
static const uint32_t CITY_COST[5] = {0u, 0u, 0u, 2u, 3u};
static const uint32_t DEVELOPMENT_COST[5] = {0u, 0u, 1u, 1u, 1u};

static inline __device__ uint32_t state_get(
    const uint32_t *states,
    uint32_t stride,
    uint32_t field,
    uint32_t lane
) {
    return states[field * stride + lane];
}

static inline __device__ void state_set(
    uint32_t *states,
    uint32_t stride,
    uint32_t field,
    uint32_t lane,
    uint32_t value
) {
    states[field * stride + lane] = value;
}

static inline __device__ uint32_t action_get(
    const uint32_t *actions,
    uint32_t stride,
    uint32_t field,
    uint32_t lane
) {
    return actions[field * stride + lane];
}

static inline __device__ uint32_t player_field(uint32_t player, uint32_t offset) {
    return STATE_PLAYERS + player * PLAYER_STRIDE + offset;
}

static inline __device__ uint32_t player_get(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t offset
) {
    return state_get(states, stride, player_field(player, offset), lane);
}

static inline __device__ void player_set(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t offset,
    uint32_t value
) {
    state_set(states, stride, player_field(player, offset), lane, value);
}

static inline __device__ uint32_t building_player(uint32_t building) {
    if (building == 0u) {
        return 0xffffffffu;
    }
    if (building <= 4u) {
        return building - 1u;
    }
    if (building <= 8u) {
        return building - 5u;
    }
    return 0xffffffffu;
}

static inline __device__ uint32_t building_multiplier(uint32_t building) {
    return building >= 5u && building <= 8u ? 2u : 1u;
}

static inline __device__ uint32_t topo_vertex_vertex_count(
    const uint32_t *topology,
    uint32_t vertex
) {
    return topology[TOPO_VERTEX_VERTEX_COUNTS + vertex];
}

static inline __device__ uint32_t topo_vertex_vertex(
    const uint32_t *topology,
    uint32_t vertex,
    uint32_t slot
) {
    return topology[TOPO_VERTEX_VERTICES + vertex * MAX_VERTEX_ADJACENCY + slot];
}

static inline __device__ uint32_t topo_vertex_edge_count(
    const uint32_t *topology,
    uint32_t vertex
) {
    return topology[TOPO_VERTEX_EDGE_COUNTS + vertex];
}

static inline __device__ uint32_t topo_vertex_edge(
    const uint32_t *topology,
    uint32_t vertex,
    uint32_t slot
) {
    return topology[TOPO_VERTEX_EDGES + vertex * MAX_VERTEX_ADJACENCY + slot];
}

static inline __device__ uint32_t topo_edge_vertex(
    const uint32_t *topology,
    uint32_t edge,
    uint32_t endpoint
) {
    return topology[TOPO_EDGE_VERTICES + edge * 2u + endpoint];
}

static inline __device__ int edge_used(uint64_t low, uint64_t high, uint32_t edge) {
    if (edge < 64u) {
        return (low & (1ull << edge)) != 0ull;
    }
    return (high & (1ull << (edge - 64u))) != 0ull;
}

static inline __device__ void edge_mark(
    uint64_t *low,
    uint64_t *high,
    uint32_t edge,
    int used
) {
    if (edge < 64u) {
        const uint64_t bit = 1ull << edge;
        *low = used ? (*low | bit) : (*low & ~bit);
    } else {
        const uint64_t bit = 1ull << (edge - 64u);
        *high = used ? (*high | bit) : (*high & ~bit);
    }
}

static inline __device__ uint32_t longest_road_from(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t root_edge,
    uint32_t root_through
) {
    uint32_t edge_stack[15];
    uint32_t through_stack[15];
    uint32_t next_slot[15];
    int depth = 0;
    uint64_t used_low = 0ull;
    uint64_t used_high = 0ull;
    edge_stack[0] = root_edge;
    through_stack[0] = root_through;
    next_slot[0] = 0u;
    edge_mark(&used_low, &used_high, root_edge, 1);
    uint32_t best = 1u;

    while (depth >= 0) {
        const uint32_t edge = edge_stack[depth];
        const uint32_t a = topo_edge_vertex(topology, edge, 0u);
        const uint32_t b = topo_edge_vertex(topology, edge, 1u);
        const uint32_t next_vertex = a == through_stack[depth] ? b : a;
        const uint32_t building = state_get(states, stride, STATE_BUILDINGS + next_vertex, lane);
        const uint32_t owner = building_player(building);
        if (owner != 0xffffffffu && owner != player) {
            edge_mark(&used_low, &used_high, edge, 0);
            --depth;
            continue;
        }

        const uint32_t count = topo_vertex_edge_count(topology, next_vertex);
        int pushed = 0;
        for (uint32_t slot = next_slot[depth]; slot < count; ++slot) {
            next_slot[depth] = slot + 1u;
            const uint32_t candidate = topo_vertex_edge(topology, next_vertex, slot);
            if (candidate == edge || edge_used(used_low, used_high, candidate)) {
                continue;
            }
            if (state_get(states, stride, STATE_ROADS + candidate, lane) != player + 1u) {
                continue;
            }
            if (depth + 1 >= 15) {
                continue;
            }
            ++depth;
            edge_stack[depth] = candidate;
            through_stack[depth] = next_vertex;
            next_slot[depth] = 0u;
            edge_mark(&used_low, &used_high, candidate, 1);
            const uint32_t length = (uint32_t)depth + 1u;
            best = length > best ? length : best;
            pushed = 1;
            break;
        }
        if (pushed) {
            continue;
        }
        edge_mark(&used_low, &used_high, edge, 0);
        --depth;
    }
    return best;
}

static inline __device__ uint32_t longest_road_length(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player
) {
    uint32_t best = 0u;
    for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
        if (state_get(states, stride, STATE_ROADS + edge, lane) != player + 1u) {
            continue;
        }
        const uint32_t a = topo_edge_vertex(topology, edge, 0u);
        const uint32_t b = topo_edge_vertex(topology, edge, 1u);
        const uint32_t from_a = longest_road_from(states, topology, stride, lane, player, edge, a);
        const uint32_t from_b = longest_road_from(states, topology, stride, lane, player, edge, b);
        const uint32_t length = from_a > from_b ? from_a : from_b;
        best = length > best ? length : best;
    }
    return best;
}

static inline __device__ void update_longest_road(
    uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane
) {
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    uint32_t lengths[MAX_PLAYERS];
    uint32_t best = 0u;
    for (uint32_t player = 0u; player < players; ++player) {
        lengths[player] = longest_road_length(states, topology, stride, lane, player);
        best = lengths[player] > best ? lengths[player] : best;
    }
    uint32_t leader_count = 0u;
    uint32_t sole_leader = 0u;
    const uint32_t old_holder_code = state_get(states, stride, STATE_LONGEST_HOLDER, lane);
    uint32_t old_holder_is_leader = 0u;
    for (uint32_t player = 0u; player < players; ++player) {
        if (lengths[player] == best && best >= 5u) {
            ++leader_count;
            sole_leader = player;
            if (old_holder_code == player + 1u) {
                old_holder_is_leader = 1u;
            }
        }
    }
    uint32_t next_holder_code = 0u;
    if (old_holder_code != 0u && old_holder_is_leader != 0u) {
        next_holder_code = old_holder_code;
    } else if (leader_count == 1u) {
        next_holder_code = sole_leader + 1u;
    }
    if (next_holder_code == old_holder_code) {
        return;
    }
    if (old_holder_code != 0u) {
        const uint32_t old_player = old_holder_code - 1u;
        player_set(states, stride, lane, old_player, PLAYER_HAS_LONGEST, 0u);
        const uint32_t vp = player_get(states, stride, lane, old_player, PLAYER_PUBLIC_VP);
        player_set(states, stride, lane, old_player, PLAYER_PUBLIC_VP, vp - 2u);
    }
    if (next_holder_code != 0u) {
        const uint32_t new_player = next_holder_code - 1u;
        player_set(states, stride, lane, new_player, PLAYER_HAS_LONGEST, 1u);
        const uint32_t vp = player_get(states, stride, lane, new_player, PLAYER_PUBLIC_VP);
        player_set(states, stride, lane, new_player, PLAYER_PUBLIC_VP, vp + 2u);
    }
    state_set(states, stride, STATE_LONGEST_HOLDER, lane, next_holder_code);
}

static inline __device__ int place_road_piece(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge
) {
    if (edge >= EDGE_COUNT) {
        return 0;
    }
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t left = player_get(states, stride, lane, player, PLAYER_ROADS_LEFT);
    if (left == 0u) {
        return 0;
    }
    player_set(states, stride, lane, player, PLAYER_ROADS_LEFT, left - 1u);
    state_set(states, stride, STATE_ROADS + edge, lane, player + 1u);
    return 1;
}

static inline __device__ int place_settlement_piece(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t vertex
) {
    if (vertex >= VERTEX_COUNT) {
        return 0;
    }
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t left = player_get(states, stride, lane, player, PLAYER_SETTLEMENTS_LEFT);
    if (left == 0u) {
        return 0;
    }
    player_set(states, stride, lane, player, PLAYER_SETTLEMENTS_LEFT, left - 1u);
    const uint32_t vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP);
    player_set(states, stride, lane, player, PLAYER_PUBLIC_VP, vp + 1u);
    state_set(states, stride, STATE_BUILDINGS + vertex, lane, player + 1u);
    return 1;
}

static inline __device__ void finish_if_won(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane
) {
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t public_vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP);
    const uint32_t hidden_vp = player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
    if (public_vp + hidden_vp >= state_get(states, stride, STATE_VICTORY_TARGET, lane)) {
        state_set(states, stride, STATE_PHASE, lane, PHASE_FINISHED);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
    }
}

static inline __device__ uint32_t state_actor(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane
) {
    const uint32_t phase = state_get(states, stride, STATE_PHASE, lane);
    if (phase == PHASE_DISCARD) {
        return state_get(states, stride, STATE_DISCARD_CURSOR, lane);
    }
    return state_get(states, stride, STATE_CURRENT_PLAYER, lane);
}

static inline __device__ uint32_t resource_total(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player
) {
    uint32_t total = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        total += player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
    }
    return total;
}

static inline __device__ int pay_current(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    const uint32_t cost[5]
) {
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (player_get(states, stride, lane, player, PLAYER_RESOURCES + resource) < cost[resource]) {
            return 0;
        }
    }
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t held = player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
        player_set(states, stride, lane, player, PLAYER_RESOURCES + resource, held - cost[resource]);
        const uint32_t bank = state_get(states, stride, STATE_BANK + resource, lane);
        state_set(states, stride, STATE_BANK + resource, lane, bank + cost[resource]);
    }
    return 1;
}

static inline __device__ uint32_t trade_ratio(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t resource
) {
    uint32_t ratio = 4u;
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        if (building_player(state_get(states, stride, STATE_BUILDINGS + vertex, lane)) != player) {
            continue;
        }
        const uint32_t port = state_get(states, stride, STATE_PORTS + vertex, lane);
        if (port == 1u) {
            ratio = ratio < 3u ? ratio : 3u;
        } else if (port == resource + 2u) {
            ratio = 2u;
        }
    }
    return ratio;
}

static inline __device__ int consume_development(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t card
) {
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    if (player_get(states, stride, lane, player, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN) != 0u) {
        return 0;
    }
    const uint32_t held = player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + card);
    const uint32_t bought = player_get(states, stride, lane, player, PLAYER_BOUGHT_DEVELOPMENT + card);
    if (held == 0u || held <= bought) {
        return 0;
    }
    player_set(states, stride, lane, player, PLAYER_DEVELOPMENT + card, held - 1u);
    const uint32_t played = state_get(states, stride, STATE_PLAYED_DEVELOPMENT + card, lane);
    state_set(states, stride, STATE_PLAYED_DEVELOPMENT + card, lane, played + 1u);
    player_set(states, stride, lane, player, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN, 1u);
    return 1;
}

static inline __device__ void update_largest_army(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane
) {
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    uint32_t best = 0u;
    for (uint32_t player = 0u; player < players; ++player) {
        const uint32_t played = player_get(states, stride, lane, player, PLAYER_PLAYED_KNIGHTS);
        best = played > best ? played : best;
    }
    uint32_t leader_count = 0u;
    uint32_t sole_leader = 0u;
    const uint32_t old_holder_code = state_get(states, stride, STATE_LARGEST_HOLDER, lane);
    uint32_t old_holder_is_leader = 0u;
    for (uint32_t player = 0u; player < players; ++player) {
        const uint32_t played = player_get(states, stride, lane, player, PLAYER_PLAYED_KNIGHTS);
        if (played == best && best >= 3u) {
            ++leader_count;
            sole_leader = player;
            if (old_holder_code == player + 1u) {
                old_holder_is_leader = 1u;
            }
        }
    }
    uint32_t next_holder_code = 0u;
    if (old_holder_code != 0u && old_holder_is_leader != 0u) {
        next_holder_code = old_holder_code;
    } else if (leader_count == 1u) {
        next_holder_code = sole_leader + 1u;
    }
    if (next_holder_code == old_holder_code) {
        return;
    }
    if (old_holder_code != 0u) {
        const uint32_t old_player = old_holder_code - 1u;
        player_set(states, stride, lane, old_player, PLAYER_HAS_LARGEST, 0u);
        const uint32_t vp = player_get(states, stride, lane, old_player, PLAYER_PUBLIC_VP);
        player_set(states, stride, lane, old_player, PLAYER_PUBLIC_VP, vp - 2u);
    }
    if (next_holder_code != 0u) {
        const uint32_t new_player = next_holder_code - 1u;
        player_set(states, stride, lane, new_player, PLAYER_HAS_LARGEST, 1u);
        const uint32_t vp = player_get(states, stride, lane, new_player, PLAYER_PUBLIC_VP);
        player_set(states, stride, lane, new_player, PLAYER_PUBLIC_VP, vp + 2u);
    }
    state_set(states, stride, STATE_LARGEST_HOLDER, lane, next_holder_code);
}

static inline __device__ void restore_robber_return_phase(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane
) {
    state_set(
        states,
        stride,
        STATE_PHASE,
        lane,
        state_get(states, stride, STATE_ROBBER_RETURN_PHASE, lane)
    );
    state_set(
        states,
        stride,
        STATE_PHASE_ARG,
        lane,
        state_get(states, stride, STATE_ROBBER_RETURN_ARG, lane)
    );
}

static inline __device__ void produce_roll(
    uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t roll
) {
    uint32_t demand[MAX_PLAYERS * 5u];
    for (uint32_t index = 0u; index < MAX_PLAYERS * 5u; ++index) {
        demand[index] = 0u;
    }
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        const uint32_t building = state_get(states, stride, STATE_BUILDINGS + vertex, lane);
        const uint32_t player = building_player(building);
        if (player == 0xffffffffu) {
            continue;
        }
        const uint32_t multiplier = building_multiplier(building);
        const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
        for (uint32_t slot = 0u; slot < count; ++slot) {
            const uint32_t hex = topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot];
            if (hex == state_get(states, stride, STATE_ROBBER_HEX, lane)) {
                continue;
            }
            if (state_get(states, stride, STATE_HEX_NUMBERS + hex, lane) != roll) {
                continue;
            }
            const uint32_t encoded_resource = state_get(states, stride, STATE_HEX_RESOURCES + hex, lane);
            if (encoded_resource == 0u) {
                continue;
            }
            const uint32_t resource = encoded_resource - 1u;
            demand[player * 5u + resource] += multiplier;
        }
    }
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        uint32_t total = 0u;
        for (uint32_t player = 0u; player < players; ++player) {
            total += demand[player * 5u + resource];
        }
        const uint32_t bank = state_get(states, stride, STATE_BANK + resource, lane);
        if (total > bank) {
            continue;
        }
        state_set(states, stride, STATE_BANK + resource, lane, bank - total);
        for (uint32_t player = 0u; player < players; ++player) {
            const uint32_t gain = demand[player * 5u + resource];
            if (gain == 0u) {
                continue;
            }
            const uint32_t held = player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
            player_set(states, stride, lane, player, PLAYER_RESOURCES + resource, held + gain);
        }
    }
}

static inline __device__ uint64_t splitmix64_next(uint64_t *state) {
    *state += 0x9e3779b97f4a7c15ull;
    uint64_t value = *state;
    value = (value ^ (value >> 30)) * 0xbf58476d1ce4e5b9ull;
    value = (value ^ (value >> 27)) * 0x94d049bb133111ebull;
    return value ^ (value >> 31);
}

static inline __device__ uint32_t rng_range(uint64_t *state, uint32_t end) {
    return (uint32_t)(splitmix64_next(state) % (uint64_t)end);
}

static inline __device__ void clear_action(
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane
) {
    for (uint32_t field = 0u; field < 8u; ++field) {
        actions[field * stride + lane] = 0u;
    }
}

static inline __device__ void write_action(
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint32_t tag,
    uint32_t arg0,
    uint32_t arg1,
    uint32_t arg2
) {
    actions[ACTION_TAG * stride + lane] = tag;
    actions[(ACTION_ARG0 + 0u) * stride + lane] = arg0;
    actions[(ACTION_ARG0 + 1u) * stride + lane] = arg1;
    actions[(ACTION_ARG0 + 2u) * stride + lane] = arg2;
}

static inline __device__ void reservoir_action(
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint64_t *rng,
    uint32_t *seen,
    uint32_t tag,
    uint32_t arg0,
    uint32_t arg1,
    uint32_t arg2
) {
    *seen += 1u;
    if (rng_range(rng, *seen) == 0u) {
        write_action(actions, stride, lane, tag, arg0, arg1, arg2);
    }
}

static inline __device__ int has_cost(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    const uint32_t cost[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (player_get(states, stride, lane, player, PLAYER_RESOURCES + resource) < cost[resource]) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ int can_place_settlement_device(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t vertex,
    int setup
) {
    if (vertex >= VERTEX_COUNT || state_get(states, stride, STATE_BUILDINGS + vertex, lane) != 0u) {
        return 0;
    }
    const uint32_t adjacent_vertices = topo_vertex_vertex_count(topology, vertex);
    for (uint32_t slot = 0u; slot < adjacent_vertices; ++slot) {
        const uint32_t neighbor = topo_vertex_vertex(topology, vertex, slot);
        if (state_get(states, stride, STATE_BUILDINGS + neighbor, lane) != 0u) {
            return 0;
        }
    }
    if (setup) {
        return 1;
    }
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t adjacent_edges = topo_vertex_edge_count(topology, vertex);
    for (uint32_t slot = 0u; slot < adjacent_edges; ++slot) {
        const uint32_t edge = topo_vertex_edge(topology, vertex, slot);
        if (state_get(states, stride, STATE_ROADS + edge, lane) == current + 1u) {
            return 1;
        }
    }
    return 0;
}

static inline __device__ uint32_t road_owner_with_extra(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge,
    uint32_t extra_edge,
    uint32_t current
) {
    if (edge == extra_edge) {
        return current + 1u;
    }
    return state_get(states, stride, STATE_ROADS + edge, lane);
}

static inline __device__ int can_build_road_device(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge,
    uint32_t extra_edge
) {
    if (edge >= EDGE_COUNT || edge == extra_edge
        || state_get(states, stride, STATE_ROADS + edge, lane) != 0u) {
        return 0;
    }
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    for (uint32_t endpoint = 0u; endpoint < 2u; ++endpoint) {
        const uint32_t vertex = topo_edge_vertex(topology, edge, endpoint);
        const uint32_t building = state_get(states, stride, STATE_BUILDINGS + vertex, lane);
        if (building != 0u) {
            if (building_player(building) == current) {
                return 1;
            }
            continue;
        }
        const uint32_t count = topo_vertex_edge_count(topology, vertex);
        for (uint32_t slot = 0u; slot < count; ++slot) {
            const uint32_t neighbor = topo_vertex_edge(topology, vertex, slot);
            if (neighbor != edge
                && road_owner_with_extra(states, stride, lane, neighbor, extra_edge, current)
                    == current + 1u) {
                return 1;
            }
        }
    }
    return 0;
}

static inline __device__ int robber_hex_allowed(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t hex
) {
    if (hex >= HEX_COUNT || hex == state_get(states, stride, STATE_ROBBER_HEX, lane)) {
        return 0;
    }
    if (state_get(states, stride, STATE_FRIENDLY_ROBBER, lane) == 0u) {
        return 1;
    }
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        const uint32_t building = state_get(states, stride, STATE_BUILDINGS + vertex, lane);
        const uint32_t owner = building_player(building);
        if (owner == 0xffffffffu
            || player_get(states, stride, lane, owner, PLAYER_PUBLIC_VP) >= 3u) {
            continue;
        }
        const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
        for (uint32_t slot = 0u; slot < count; ++slot) {
            if (topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot] == hex) {
                return 0;
            }
        }
    }
    return 1;
}

static inline __device__ uint32_t robber_victim_mask(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t hex
) {
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    uint32_t victims = 0u;
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        const uint32_t building = state_get(states, stride, STATE_BUILDINGS + vertex, lane);
        const uint32_t owner = building_player(building);
        if (owner == 0xffffffffu || owner == current || resource_total(states, stride, lane, owner) == 0u) {
            continue;
        }
        const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
        for (uint32_t slot = 0u; slot < count; ++slot) {
            if (topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot] == hex) {
                victims |= 1u << owner;
                break;
            }
        }
    }
    return victims;
}

static inline __device__ void reservoir_robber_actions(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint64_t *rng,
    uint32_t *seen,
    uint32_t tag
) {
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    for (uint32_t hex = 0u; hex < HEX_COUNT; ++hex) {
        if (!robber_hex_allowed(states, topology, stride, lane, hex)) {
            continue;
        }
        const uint32_t victims = robber_victim_mask(states, topology, stride, lane, hex);
        if (victims == 0u) {
            reservoir_action(actions, stride, lane, rng, seen, tag, hex, 0u, 0u);
            continue;
        }
        for (uint32_t player = 0u; player < players; ++player) {
            if ((victims & (1u << player)) != 0u) {
                reservoir_action(actions, stride, lane, rng, seen, tag, hex, player + 1u, 0u);
            }
        }
    }
}

static inline __device__ int development_playable(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t card
) {
    return player_get(states, stride, lane, player, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN) == 0u
        && player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + card)
            > player_get(states, stride, lane, player, PLAYER_BOUGHT_DEVELOPMENT + card);
}

static inline __device__ void generate_rollout_action_lane(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint64_t *rng_states,
    uint32_t stride,
    uint32_t lane
) {
    clear_action(actions, stride, lane);
    uint64_t rng = rng_states[lane];
    const uint32_t phase = state_get(states, stride, STATE_PHASE, lane);
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    uint32_t seen = 0u;

    if (phase == PHASE_SETUP_SETTLEMENT) {
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            if (can_place_settlement_device(states, topology, stride, lane, vertex, 1)) {
                reservoir_action(actions, stride, lane, &rng, &seen, ACTION_PLACE_SETTLEMENT, vertex, 0u, 0u);
            }
        }
    } else if (phase == PHASE_SETUP_ROAD) {
        const uint32_t settlement = state_get(states, stride, STATE_PHASE_ARG, lane);
        const uint32_t edge_count = topo_vertex_edge_count(topology, settlement);
        for (uint32_t slot = 0u; slot < edge_count; ++slot) {
            const uint32_t edge = topo_vertex_edge(topology, settlement, slot);
            if (state_get(states, stride, STATE_ROADS + edge, lane) == 0u) {
                reservoir_action(actions, stride, lane, &rng, &seen, ACTION_PLACE_ROAD, edge, 0u, 0u);
            }
        }
    } else if (phase == PHASE_PRE_ROLL) {
        reservoir_action(actions, stride, lane, &rng, &seen, ACTION_ROLL, 0u, 0u, 0u);
        if (development_playable(states, stride, lane, current, 0u)) {
            reservoir_robber_actions(
                states, topology, actions, stride, lane, &rng, &seen, ACTION_PLAY_KNIGHT
            );
        }
    } else if (phase == PHASE_ROLL_CHANCE) {
        const uint32_t roll = rng_range(&rng, 6u) + rng_range(&rng, 6u) + 2u;
        write_action(actions, stride, lane, ACTION_RESOLVE_ROLL, roll, 0u, 0u);
        seen = 1u;
    } else if (phase == PHASE_DISCARD) {
        const uint32_t player = state_get(states, stride, STATE_DISCARD_CURSOR, lane);
        const uint32_t required = state_get(states, stride, STATE_DISCARD_REMAINING + player, lane);
        uint32_t remaining[5];
        uint32_t discard[5] = {0u, 0u, 0u, 0u, 0u};
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            remaining[resource] = player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
        }
        for (uint32_t card = 0u; card < required; ++card) {
            uint32_t total = 0u;
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                total += remaining[resource];
            }
            if (total == 0u) {
                break;
            }
            uint32_t target = rng_range(&rng, total);
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                if (target < remaining[resource]) {
                    --remaining[resource];
                    ++discard[resource];
                    break;
                }
                target -= remaining[resource];
            }
        }
        write_action(actions, stride, lane, ACTION_DISCARD, discard[0], discard[1], discard[2]);
        actions[(ACTION_ARG0 + 3u) * stride + lane] = discard[3];
        actions[(ACTION_ARG0 + 4u) * stride + lane] = discard[4];
        seen = 1u;
    } else if (phase == PHASE_MOVE_ROBBER) {
        reservoir_robber_actions(
            states, topology, actions, stride, lane, &rng, &seen, ACTION_MOVE_ROBBER
        );
    } else if (phase == PHASE_RESOLVE_STEAL) {
        const uint32_t victim = state_get(states, stride, STATE_PHASE_ARG, lane);
        const uint32_t total = resource_total(states, stride, lane, victim);
        if (total > 0u) {
            uint32_t target = rng_range(&rng, total);
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                const uint32_t held = player_get(states, stride, lane, victim, PLAYER_RESOURCES + resource);
                if (target < held) {
                    write_action(actions, stride, lane, ACTION_RESOLVE_STEAL, victim, resource, 0u);
                    seen = 1u;
                    break;
                }
                target -= held;
            }
        }
    } else if (phase == PHASE_MAIN) {
        reservoir_action(actions, stride, lane, &rng, &seen, ACTION_END_TURN, 0u, 0u, 0u);

        if (player_get(states, stride, lane, current, PLAYER_ROADS_LEFT) > 0u
            && has_cost(states, stride, lane, current, ROAD_COST)) {
            for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
                if (can_build_road_device(states, topology, stride, lane, edge, 0xffffffffu)) {
                    reservoir_action(actions, stride, lane, &rng, &seen, ACTION_BUILD_ROAD, edge, 0u, 0u);
                }
            }
        }
        if (player_get(states, stride, lane, current, PLAYER_SETTLEMENTS_LEFT) > 0u
            && has_cost(states, stride, lane, current, SETTLEMENT_COST)) {
            for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
                if (can_place_settlement_device(states, topology, stride, lane, vertex, 0)) {
                    reservoir_action(actions, stride, lane, &rng, &seen, ACTION_BUILD_SETTLEMENT, vertex, 0u, 0u);
                }
            }
        }
        if (player_get(states, stride, lane, current, PLAYER_CITIES_LEFT) > 0u
            && has_cost(states, stride, lane, current, CITY_COST)) {
            for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
                if (state_get(states, stride, STATE_BUILDINGS + vertex, lane) == current + 1u) {
                    reservoir_action(actions, stride, lane, &rng, &seen, ACTION_BUILD_CITY, vertex, 0u, 0u);
                }
            }
        }
        uint32_t deck_total = 0u;
        for (uint32_t card = 0u; card < 5u; ++card) {
            deck_total += state_get(states, stride, STATE_DEVELOPMENT_DECK + card, lane);
        }
        if (deck_total > 0u && has_cost(states, stride, lane, current, DEVELOPMENT_COST)) {
            reservoir_action(actions, stride, lane, &rng, &seen, ACTION_BUY_DEVELOPMENT, 0u, 0u, 0u);
        }

        uint32_t ratios[5];
        for (uint32_t give = 0u; give < 5u; ++give) {
            ratios[give] = trade_ratio(states, stride, lane, current, give);
            if (player_get(states, stride, lane, current, PLAYER_RESOURCES + give) < ratios[give]) {
                continue;
            }
            for (uint32_t receive = 0u; receive < 5u; ++receive) {
                if (give != receive && state_get(states, stride, STATE_BANK + receive, lane) > 0u) {
                    reservoir_action(
                        actions, stride, lane, &rng, &seen,
                        ACTION_MARITIME_TRADE, give, receive, ratios[give]
                    );
                }
            }
        }

        if (development_playable(states, stride, lane, current, 0u)) {
            reservoir_robber_actions(
                states, topology, actions, stride, lane, &rng, &seen, ACTION_PLAY_KNIGHT
            );
        }
        if (development_playable(states, stride, lane, current, 2u)
            && player_get(states, stride, lane, current, PLAYER_ROADS_LEFT) > 0u) {
            uint32_t first_seen = 0u;
            uint32_t first = 0xffffffffu;
            for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
                if (!can_build_road_device(states, topology, stride, lane, edge, 0xffffffffu)) {
                    continue;
                }
                ++first_seen;
                if (rng_range(&rng, first_seen) == 0u) {
                    first = edge;
                }
            }
            if (first != 0xffffffffu) {
                uint32_t second_code = 0u;
                if (player_get(states, stride, lane, current, PLAYER_ROADS_LEFT) > 1u) {
                    uint32_t second_seen = 0u;
                    uint32_t second = 0xffffffffu;
                    for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
                        if (!can_build_road_device(states, topology, stride, lane, edge, first)) {
                            continue;
                        }
                        ++second_seen;
                        if (rng_range(&rng, second_seen) == 0u) {
                            second = edge;
                        }
                    }
                    if (second != 0xffffffffu) {
                        second_code = second + 1u;
                    }
                }
                reservoir_action(
                    actions, stride, lane, &rng, &seen,
                    ACTION_PLAY_ROAD_BUILDING, first, second_code, 0u
                );
            }
        }
        if (development_playable(states, stride, lane, current, 3u)) {
            for (uint32_t first = 0u; first < 5u; ++first) {
                for (uint32_t second = first; second < 5u; ++second) {
                    const uint32_t needed = first == second ? 2u : 1u;
                    if (state_get(states, stride, STATE_BANK + first, lane) >= needed
                        && state_get(states, stride, STATE_BANK + second, lane) > 0u) {
                        reservoir_action(
                            actions, stride, lane, &rng, &seen,
                            ACTION_PLAY_YEAR_OF_PLENTY, first, second, 0u
                        );
                    }
                }
            }
        }
        if (development_playable(states, stride, lane, current, 4u)) {
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                reservoir_action(
                    actions, stride, lane, &rng, &seen,
                    ACTION_PLAY_MONOPOLY, resource, 0u, 0u
                );
            }
        }
    } else if (phase == PHASE_DEVELOPMENT_CHANCE) {
        uint32_t total = 0u;
        for (uint32_t card = 0u; card < 5u; ++card) {
            total += state_get(states, stride, STATE_DEVELOPMENT_DECK + card, lane);
        }
        if (total > 0u) {
            uint32_t target = rng_range(&rng, total);
            for (uint32_t card = 0u; card < 5u; ++card) {
                const uint32_t held = state_get(states, stride, STATE_DEVELOPMENT_DECK + card, lane);
                if (target < held) {
                    write_action(actions, stride, lane, ACTION_RESOLVE_DEVELOPMENT, card, 0u, 0u);
                    seen = 1u;
                    break;
                }
                target -= held;
            }
        }
    } else if (phase == PHASE_FINISHED) {
        write_action(actions, stride, lane, 255u, 0u, 0u, 0u);
        seen = 1u;
    }

    // A no-player-trades state should always have at least one action unless it
    // is invalid. Leave tag 254 as a detectable sentinel instead of fabricating
    // a legal transition.
    if (seen == 0u) {
        write_action(actions, stride, lane, 254u, 0u, 0u, 0u);
    }
    rng_states[lane] = rng;
}

static inline __device__ void apply_transition_lane(
    uint32_t *states,
    const uint32_t *topology,
    const uint32_t *actions,
    uint32_t *status,
    uint32_t stride,
    uint32_t lane
) {
    if (status[lane] != STATUS_OK) {
        return;
    }
    if (state_get(states, stride, STATE_PLAYER_TRADES_ENABLED, lane) != 0u) {
        status[lane] = STATUS_INVALID_STATE;
        return;
    }

    const uint32_t tag = action_get(actions, stride, ACTION_TAG, lane);
    const uint32_t phase = state_get(states, stride, STATE_PHASE, lane);
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);

    if (tag == 255u && phase == PHASE_FINISHED) {
        return;
    }

    if (tag == ACTION_PLACE_SETTLEMENT) {
        if (phase != PHASE_SETUP_SETTLEMENT) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t vertex = action_get(actions, stride, ACTION_ARG0, lane);
        if (!place_settlement_piece(states, stride, lane, vertex)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        if (state_get(states, stride, STATE_SETUP_STEP, lane) >= players) {
            const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
            for (uint32_t slot = 0u; slot < count; ++slot) {
                const uint32_t hex = topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot];
                const uint32_t encoded_resource = state_get(states, stride, STATE_HEX_RESOURCES + hex, lane);
                if (encoded_resource == 0u) {
                    continue;
                }
                const uint32_t resource = encoded_resource - 1u;
                const uint32_t bank = state_get(states, stride, STATE_BANK + resource, lane);
                if (bank == 0u) {
                    continue;
                }
                state_set(states, stride, STATE_BANK + resource, lane, bank - 1u);
                const uint32_t held = player_get(states, stride, lane, current, PLAYER_RESOURCES + resource);
                player_set(states, stride, lane, current, PLAYER_RESOURCES + resource, held + 1u);
            }
        }
        state_set(states, stride, STATE_PHASE, lane, PHASE_SETUP_ROAD);
        state_set(states, stride, STATE_PHASE_ARG, lane, vertex);
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_PLACE_ROAD) {
        if (phase != PHASE_SETUP_ROAD) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t edge = action_get(actions, stride, ACTION_ARG0, lane);
        if (!place_road_piece(states, stride, lane, edge)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t setup_step = state_get(states, stride, STATE_SETUP_STEP, lane) + 1u;
        state_set(states, stride, STATE_SETUP_STEP, lane, setup_step);
        const uint32_t total = players * 2u;
        if (setup_step >= total) {
            state_set(states, stride, STATE_CURRENT_PLAYER, lane, 0u);
            state_set(states, stride, STATE_PHASE, lane, PHASE_PRE_ROLL);
            state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
            state_set(states, stride, STATE_TURN, lane, 1u);
        } else {
            const uint32_t next = setup_step < players ? setup_step : total - setup_step - 1u;
            state_set(states, stride, STATE_CURRENT_PLAYER, lane, next);
            state_set(states, stride, STATE_PHASE, lane, PHASE_SETUP_SETTLEMENT);
            state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        }
        return;
    }

    if (tag == ACTION_ROLL) {
        if (phase != PHASE_PRE_ROLL) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        state_set(states, stride, STATE_PHASE, lane, PHASE_ROLL_CHANCE);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        return;
    }

    if (tag == ACTION_RESOLVE_ROLL) {
        if (phase != PHASE_ROLL_CHANCE) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t roll = action_get(actions, stride, ACTION_ARG0, lane);
        if (roll < 2u || roll > 12u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        state_set(states, stride, STATE_LAST_ROLL, lane, roll);
        if (roll == 7u) {
            for (uint32_t player = 0u; player < MAX_PLAYERS; ++player) {
                state_set(states, stride, STATE_DISCARD_REMAINING + player, lane, 0u);
            }
            const uint32_t discard_limit = state_get(states, stride, STATE_DISCARD_LIMIT, lane);
            uint32_t next_discarder = 0xffffffffu;
            for (uint32_t player = 0u; player < players; ++player) {
                const uint32_t total = resource_total(states, stride, lane, player);
                if (total > discard_limit) {
                    state_set(states, stride, STATE_DISCARD_REMAINING + player, lane, total / 2u);
                    if (next_discarder == 0xffffffffu) {
                        next_discarder = player;
                    }
                }
            }
            if (next_discarder != 0xffffffffu) {
                state_set(states, stride, STATE_DISCARD_CURSOR, lane, next_discarder);
                state_set(states, stride, STATE_PHASE, lane, PHASE_DISCARD);
                state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
            } else {
                state_set(states, stride, STATE_ROBBER_RETURN_PHASE, lane, PHASE_MAIN);
                state_set(states, stride, STATE_ROBBER_RETURN_ARG, lane, 0u);
                state_set(states, stride, STATE_PHASE, lane, PHASE_MOVE_ROBBER);
                state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
            }
        } else {
            produce_roll(states, topology, stride, lane, roll);
            state_set(states, stride, STATE_PHASE, lane, PHASE_MAIN);
            state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        }
        return;
    }

    if (tag == ACTION_DISCARD) {
        if (phase != PHASE_DISCARD) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t player = state_get(states, stride, STATE_DISCARD_CURSOR, lane);
        const uint32_t required = state_get(states, stride, STATE_DISCARD_REMAINING + player, lane);
        uint32_t total = 0u;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t cards = action_get(actions, stride, ACTION_ARG0 + resource, lane);
            if (cards > player_get(states, stride, lane, player, PLAYER_RESOURCES + resource)) {
                status[lane] = STATUS_INVALID_ACTION;
                return;
            }
            total += cards;
        }
        if (total != required) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t cards = action_get(actions, stride, ACTION_ARG0 + resource, lane);
            const uint32_t held = player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
            player_set(states, stride, lane, player, PLAYER_RESOURCES + resource, held - cards);
            const uint32_t bank = state_get(states, stride, STATE_BANK + resource, lane);
            state_set(states, stride, STATE_BANK + resource, lane, bank + cards);
        }
        state_set(states, stride, STATE_DISCARD_REMAINING + player, lane, 0u);
        uint32_t next_discarder = 0xffffffffu;
        for (uint32_t candidate = player + 1u; candidate < players; ++candidate) {
            if (state_get(states, stride, STATE_DISCARD_REMAINING + candidate, lane) > 0u) {
                next_discarder = candidate;
                break;
            }
        }
        if (next_discarder != 0xffffffffu) {
            state_set(states, stride, STATE_DISCARD_CURSOR, lane, next_discarder);
        } else {
            state_set(states, stride, STATE_ROBBER_RETURN_PHASE, lane, PHASE_MAIN);
            state_set(states, stride, STATE_ROBBER_RETURN_ARG, lane, 0u);
            state_set(states, stride, STATE_PHASE, lane, PHASE_MOVE_ROBBER);
            state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        }
        return;
    }

    if (tag == ACTION_MOVE_ROBBER) {
        if (phase != PHASE_MOVE_ROBBER) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t hex = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t victim_code = action_get(actions, stride, ACTION_ARG0 + 1u, lane);
        if (hex >= HEX_COUNT || hex == state_get(states, stride, STATE_ROBBER_HEX, lane)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        state_set(states, stride, STATE_ROBBER_HEX, lane, hex);
        if (victim_code == 0u) {
            restore_robber_return_phase(states, stride, lane);
        } else {
            const uint32_t victim = victim_code - 1u;
            if (victim >= players || victim == current) {
                status[lane] = STATUS_INVALID_ACTION;
                return;
            }
            state_set(states, stride, STATE_PHASE, lane, PHASE_RESOLVE_STEAL);
            state_set(states, stride, STATE_PHASE_ARG, lane, victim);
        }
        return;
    }

    if (tag == ACTION_RESOLVE_STEAL) {
        const uint32_t victim = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t resource = action_get(actions, stride, ACTION_ARG0 + 1u, lane);
        if (phase != PHASE_RESOLVE_STEAL
            || state_get(states, stride, STATE_PHASE_ARG, lane) != victim) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        if (victim >= players || resource >= 5u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t victim_cards = player_get(states, stride, lane, victim, PLAYER_RESOURCES + resource);
        if (victim_cards == 0u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        player_set(states, stride, lane, victim, PLAYER_RESOURCES + resource, victim_cards - 1u);
        const uint32_t held = player_get(states, stride, lane, current, PLAYER_RESOURCES + resource);
        player_set(states, stride, lane, current, PLAYER_RESOURCES + resource, held + 1u);
        restore_robber_return_phase(states, stride, lane);
        return;
    }

    if (tag == ACTION_BUILD_ROAD) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t edge = action_get(actions, stride, ACTION_ARG0, lane);
        if (!pay_current(states, stride, lane, ROAD_COST)
            || !place_road_piece(states, stride, lane, edge)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        update_longest_road(states, topology, stride, lane);
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_BUILD_SETTLEMENT) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t vertex = action_get(actions, stride, ACTION_ARG0, lane);
        if (!pay_current(states, stride, lane, SETTLEMENT_COST)
            || !place_settlement_piece(states, stride, lane, vertex)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        update_longest_road(states, topology, stride, lane);
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_BUILD_CITY) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t vertex = action_get(actions, stride, ACTION_ARG0, lane);
        if (vertex >= VERTEX_COUNT
            || state_get(states, stride, STATE_BUILDINGS + vertex, lane) != current + 1u
            || player_get(states, stride, lane, current, PLAYER_CITIES_LEFT) == 0u
            || !pay_current(states, stride, lane, CITY_COST)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t cities = player_get(states, stride, lane, current, PLAYER_CITIES_LEFT);
        const uint32_t settlements = player_get(states, stride, lane, current, PLAYER_SETTLEMENTS_LEFT);
        const uint32_t vp = player_get(states, stride, lane, current, PLAYER_PUBLIC_VP);
        player_set(states, stride, lane, current, PLAYER_CITIES_LEFT, cities - 1u);
        player_set(states, stride, lane, current, PLAYER_SETTLEMENTS_LEFT, settlements + 1u);
        player_set(states, stride, lane, current, PLAYER_PUBLIC_VP, vp + 1u);
        state_set(states, stride, STATE_BUILDINGS + vertex, lane, current + 5u);
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_BUY_DEVELOPMENT) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        uint32_t deck_total = 0u;
        for (uint32_t card = 0u; card < 5u; ++card) {
            deck_total += state_get(states, stride, STATE_DEVELOPMENT_DECK + card, lane);
        }
        if (deck_total == 0u || !pay_current(states, stride, lane, DEVELOPMENT_COST)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        state_set(states, stride, STATE_PHASE, lane, PHASE_DEVELOPMENT_CHANCE);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        return;
    }

    if (tag == ACTION_RESOLVE_DEVELOPMENT) {
        if (phase != PHASE_DEVELOPMENT_CHANCE) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t card = action_get(actions, stride, ACTION_ARG0, lane);
        if (card >= 5u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t deck = state_get(states, stride, STATE_DEVELOPMENT_DECK + card, lane);
        if (deck == 0u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        state_set(states, stride, STATE_DEVELOPMENT_DECK + card, lane, deck - 1u);
        const uint32_t held = player_get(states, stride, lane, current, PLAYER_DEVELOPMENT + card);
        const uint32_t bought = player_get(states, stride, lane, current, PLAYER_BOUGHT_DEVELOPMENT + card);
        player_set(states, stride, lane, current, PLAYER_DEVELOPMENT + card, held + 1u);
        player_set(states, stride, lane, current, PLAYER_BOUGHT_DEVELOPMENT + card, bought + 1u);
        state_set(states, stride, STATE_PHASE, lane, PHASE_MAIN);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_PLAY_KNIGHT) {
        if (phase != PHASE_PRE_ROLL && phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t return_phase = phase;
        const uint32_t return_arg = state_get(states, stride, STATE_PHASE_ARG, lane);
        if (!consume_development(states, stride, lane, 0u)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t knights = player_get(states, stride, lane, current, PLAYER_PLAYED_KNIGHTS);
        player_set(states, stride, lane, current, PLAYER_PLAYED_KNIGHTS, knights + 1u);
        update_largest_army(states, stride, lane);
        state_set(states, stride, STATE_ROBBER_RETURN_PHASE, lane, return_phase);
        state_set(states, stride, STATE_ROBBER_RETURN_ARG, lane, return_arg);
        const uint32_t hex = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t victim_code = action_get(actions, stride, ACTION_ARG0 + 1u, lane);
        if (hex >= HEX_COUNT || hex == state_get(states, stride, STATE_ROBBER_HEX, lane)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        state_set(states, stride, STATE_ROBBER_HEX, lane, hex);
        if (victim_code == 0u) {
            restore_robber_return_phase(states, stride, lane);
        } else {
            const uint32_t victim = victim_code - 1u;
            if (victim >= players || victim == current) {
                status[lane] = STATUS_INVALID_ACTION;
                return;
            }
            state_set(states, stride, STATE_PHASE, lane, PHASE_RESOLVE_STEAL);
            state_set(states, stride, STATE_PHASE_ARG, lane, victim);
        }
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_PLAY_ROAD_BUILDING) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        if (!consume_development(states, stride, lane, 2u)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t first = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t second_code = action_get(actions, stride, ACTION_ARG0 + 1u, lane);
        if (!place_road_piece(states, stride, lane, first)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        update_longest_road(states, topology, stride, lane);
        if (second_code != 0u) {
            const uint32_t second = second_code - 1u;
            if (!place_road_piece(states, stride, lane, second)) {
                status[lane] = STATUS_INVALID_ACTION;
                return;
            }
            update_longest_road(states, topology, stride, lane);
        }
        finish_if_won(states, stride, lane);
        return;
    }

    if (tag == ACTION_PLAY_YEAR_OF_PLENTY) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t first = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t second = action_get(actions, stride, ACTION_ARG0 + 1u, lane);
        if (first >= 5u || second >= 5u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t first_needed = first == second ? 2u : 1u;
        if (state_get(states, stride, STATE_BANK + first, lane) < first_needed
            || state_get(states, stride, STATE_BANK + second, lane) == 0u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        if (!consume_development(states, stride, lane, 3u)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        state_set(
            states,
            stride,
            STATE_BANK + first,
            lane,
            state_get(states, stride, STATE_BANK + first, lane) - 1u
        );
        state_set(
            states,
            stride,
            STATE_BANK + second,
            lane,
            state_get(states, stride, STATE_BANK + second, lane) - 1u
        );
        player_set(
            states,
            stride,
            lane,
            current,
            PLAYER_RESOURCES + first,
            player_get(states, stride, lane, current, PLAYER_RESOURCES + first) + 1u
        );
        player_set(
            states,
            stride,
            lane,
            current,
            PLAYER_RESOURCES + second,
            player_get(states, stride, lane, current, PLAYER_RESOURCES + second) + 1u
        );
        return;
    }

    if (tag == ACTION_PLAY_MONOPOLY) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t resource = action_get(actions, stride, ACTION_ARG0, lane);
        if (resource >= 5u || !consume_development(states, stride, lane, 4u)) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        uint32_t total = 0u;
        for (uint32_t player = 0u; player < players; ++player) {
            if (player == current) {
                continue;
            }
            total += player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
            player_set(states, stride, lane, player, PLAYER_RESOURCES + resource, 0u);
        }
        const uint32_t held = player_get(states, stride, lane, current, PLAYER_RESOURCES + resource);
        player_set(states, stride, lane, current, PLAYER_RESOURCES + resource, held + total);
        return;
    }

    if (tag == ACTION_MARITIME_TRADE) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t give = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t receive = action_get(actions, stride, ACTION_ARG0 + 1u, lane);
        const uint32_t ratio = action_get(actions, stride, ACTION_ARG0 + 2u, lane);
        if (give >= 5u || receive >= 5u || give == receive) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t actual = trade_ratio(states, stride, lane, current, give);
        const uint32_t held = player_get(states, stride, lane, current, PLAYER_RESOURCES + give);
        const uint32_t bank_receive = state_get(states, stride, STATE_BANK + receive, lane);
        if (ratio != actual || held < ratio || bank_receive == 0u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        player_set(states, stride, lane, current, PLAYER_RESOURCES + give, held - ratio);
        state_set(
            states,
            stride,
            STATE_BANK + give,
            lane,
            state_get(states, stride, STATE_BANK + give, lane) + ratio
        );
        state_set(states, stride, STATE_BANK + receive, lane, bank_receive - 1u);
        player_set(
            states,
            stride,
            lane,
            current,
            PLAYER_RESOURCES + receive,
            player_get(states, stride, lane, current, PLAYER_RESOURCES + receive) + 1u
        );
        return;
    }

    if (tag == ACTION_END_TURN) {
        if (phase != PHASE_MAIN) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        for (uint32_t card = 0u; card < 5u; ++card) {
            player_set(states, stride, lane, current, PLAYER_BOUGHT_DEVELOPMENT + card, 0u);
        }
        player_set(states, stride, lane, current, PLAYER_PLAYED_DEVELOPMENT_THIS_TURN, 0u);
        state_set(states, stride, STATE_DOMESTIC_TRADE_USED, lane, 0u);
        state_set(states, stride, STATE_DOMESTIC_TRADE_COUNT, lane, 0u);
        state_set(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane, 0u);
        state_set(states, stride, STATE_CURRENT_PLAYER, lane, (current + 1u) % players);
        state_set(states, stride, STATE_TURN, lane, state_get(states, stride, STATE_TURN, lane) + 1u);
        state_set(states, stride, STATE_LAST_ROLL, lane, 0u);
        state_set(states, stride, STATE_PHASE, lane, PHASE_PRE_ROLL);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        return;
    }

    status[lane] = STATUS_UNSUPPORTED_ACTION;
}

extern "C" __global__ void generate_rollout_actions_batch_kernel(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint64_t *rng_states,
    uint32_t stride,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    generate_rollout_action_lane(states, topology, actions, rng_states, stride, lane);
}

extern "C" __global__ void apply_transition_batch_kernel(
    uint32_t *states,
    const uint32_t *topology,
    const uint32_t *actions,
    uint32_t *status,
    uint32_t stride,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    apply_transition_lane(states, topology, actions, status, stride, lane);
}

extern "C" __global__ void run_rollout_steps_kernel(
    uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint32_t *status,
    uint64_t *rng_states,
    uint32_t stride,
    uint32_t count,
    uint32_t steps
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    for (uint32_t step = 0u; step < steps && status[lane] == STATUS_OK; ++step) {
        generate_rollout_action_lane(states, topology, actions, rng_states, stride, lane);
        apply_transition_lane(states, topology, actions, status, stride, lane);
        if (state_get(states, stride, STATE_PHASE, lane) == PHASE_FINISHED) {
            break;
        }
    }
}

extern "C" __global__ void summarize_games_kernel(
    const uint32_t *states,
    uint32_t *summaries,
    uint32_t stride,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    const uint32_t phase = state_get(states, stride, STATE_PHASE, lane);
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    const uint32_t target = state_get(states, stride, STATE_VICTORY_TARGET, lane);
    const uint32_t terminal = phase == PHASE_FINISHED ? 1u : 0u;
    uint32_t winner = 0u;
    summaries[0u * stride + lane] = terminal;
    summaries[2u * stride + lane] = state_get(states, stride, STATE_TURN, lane);
    for (uint32_t player = 0u; player < MAX_PLAYERS; ++player) {
        uint32_t vp = 0u;
        if (player < players) {
            vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP)
                + player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
            if (terminal != 0u && winner == 0u && vp >= target) {
                winner = player + 1u;
            }
        }
        summaries[(3u + player) * stride + lane] = vp;
    }
    summaries[1u * stride + lane] = winner;
}

extern "C" __global__ void expand_root_rollouts_kernel(
    const uint32_t *base_states,
    uint32_t base_stride,
    const uint32_t *topology,
    const uint32_t *root_actions,
    const uint32_t *root_base_indices,
    uint32_t root_count,
    uint32_t chunk_rollouts_per_action,
    uint32_t total_rollouts_per_action,
    uint32_t rollout_offset,
    uint32_t *states,
    uint32_t *actions,
    uint32_t *status,
    uint64_t *rng_states,
    uint32_t stride,
    uint64_t seed,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count || chunk_rollouts_per_action == 0u) {
        return;
    }
    const uint32_t root = lane / chunk_rollouts_per_action;
    if (root >= root_count) {
        return;
    }
    const uint32_t base = root_base_indices[root];
    for (uint32_t field = 0u; field < STATE_WORDS; ++field) {
        states[field * stride + lane] = base_states[field * base_stride + base];
    }
    for (uint32_t field = 0u; field < 8u; ++field) {
        actions[field * stride + lane] = root_actions[field * root_count + root];
    }
    status[lane] = STATUS_OK;
    const uint32_t local_rollout = lane % chunk_rollouts_per_action;
    const uint64_t global_rollout = (uint64_t)root * (uint64_t)total_rollouts_per_action
        + (uint64_t)rollout_offset
        + (uint64_t)local_rollout;
    rng_states[lane] = seed + (global_rollout + 1ull) * 0x9e3779b97f4a7c15ull;
    apply_transition_lane(states, topology, actions, status, stride, lane);
}

extern "C" __global__ void reduce_root_rollouts_kernel(
    const uint32_t *states,
    const uint32_t *status,
    const uint32_t *base_states,
    const uint32_t *root_base_indices,
    uint64_t *stats,
    uint32_t stride,
    uint32_t base_stride,
    uint32_t root_count,
    uint32_t chunk_rollouts_per_action,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count || chunk_rollouts_per_action == 0u) {
        return;
    }
    const uint32_t root = lane / chunk_rollouts_per_action;
    if (root >= root_count) {
        return;
    }
    atomicAdd(&stats[0u * root_count + root], 1ull);
    if (status[lane] != STATUS_OK) {
        atomicAdd(&stats[1u * root_count + root], 1ull);
        return;
    }

    const uint32_t base = root_base_indices[root];
    const uint32_t actor = state_actor(base_states, base_stride, base);
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    const uint32_t target = state_get(states, stride, STATE_VICTORY_TARGET, lane);
    const uint32_t terminal = state_get(states, stride, STATE_PHASE, lane) == PHASE_FINISHED ? 1u : 0u;
    uint32_t actor_vp = 0u;
    uint32_t best_opponent_vp = 0u;
    uint32_t winner = 0xffffffffu;
    for (uint32_t player = 0u; player < players; ++player) {
        const uint32_t vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP)
            + player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
        if (player == actor) {
            actor_vp = vp;
        } else if (vp > best_opponent_vp) {
            best_opponent_vp = vp;
        }
        if (terminal != 0u && winner == 0xffffffffu && vp >= target) {
            winner = player;
        }
    }
    if (terminal != 0u) {
        atomicAdd(&stats[2u * root_count + root], 1ull);
        if (winner == actor) {
            atomicAdd(&stats[3u * root_count + root], 1ull);
        }
    }
    atomicAdd(
        &stats[4u * root_count + root],
        (uint64_t)state_get(states, stride, STATE_TURN, lane)
    );
    atomicAdd(&stats[5u * root_count + root], (uint64_t)actor_vp);
    atomicAdd(&stats[6u * root_count + root], (uint64_t)best_opponent_vp);
}
