// First GPU-resident Catan transition kernel.
//
// States use a field-major (SoA) layout: field * stride + lane. The buffer is
// mutated in place and remains resident across successive transition launches.
// The resident lane supports the standard 2-4 player rules, including optional
// player-to-player trading, so legality, transitions, rollouts, and arena games
// can remain on device after the initial state upload.
typedef unsigned short uint16_t;
typedef unsigned int uint32_t;
typedef unsigned long long uint64_t;

#define MAX_PLAYERS 4u
#define HEX_COUNT 19u
#define VERTEX_COUNT 54u
#define EDGE_COUNT 72u
#define MAX_VERTEX_ADJACENCY 3u
#define SEED_INDEX_MIX 0xd1342543de82ef95ull
#define ROOT_RNG_DOMAIN 0xa4093822299f31d0ull
#define ROOT_CHANCE_RNG_DOMAIN 0x082efa98ec4e6c89ull
#define CANDIDATE_PROPOSAL_RNG_DOMAIN 0x9216d5d98979fb1bull
#define CANDIDATE_PROPOSAL_CHANCE_RNG_DOMAIN 0xd1310ba698dfb5acull

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
#define TRADE_STRIDE 15u
#define TRADE_PRESENT 0u
#define TRADE_CREATOR 1u
#define TRADE_RECIPIENTS 2u
#define TRADE_GIVE 3u
#define TRADE_RECEIVE 8u
#define TRADE_ACCEPTED 13u
#define TRADE_REJECTED 14u
#define STATE_TRADE 23u
#define STATE_LAST_REJECTED_TRADE 38u
#define STATE_BANK 53u
#define STATE_DEVELOPMENT_DECK 58u
#define STATE_PLAYED_DEVELOPMENT 63u
#define STATE_DISCARD_REMAINING 68u
#define STATE_HEX_RESOURCES 72u
#define STATE_HEX_NUMBERS 91u
#define STATE_PORTS 110u
#define STATE_BUILDINGS 164u
#define STATE_ROADS 218u
#define STATE_PLAYERS 290u
#define PLAYER_STRIDE 28u
#define STATE_DOMESTIC_TRADE_DISABLED 402u
#define STATE_DOMESTIC_TRADE_EMBARGOES 403u
#define STATE_WORDS 404u

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
#define ACTION_WORDS 12u

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
#define ACTION_OFFER_TRADE 18u
#define ACTION_RESPOND_TRADE 19u
#define ACTION_COUNTER_TRADE 20u
#define ACTION_CONFIRM_TRADE 21u
#define ACTION_CANCEL_TRADE 22u

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
static const uint32_t ROAD_CUT_POLICY_ROAD_LOSS_WEIGHT = 180u;
static const uint32_t ROAD_CUT_POLICY_AWARD_LOSS_WEIGHT = 540u;
static const uint32_t DOMESTIC_PLAN_COSTS[6][5] = {
    {1u, 1u, 0u, 0u, 0u},
    {1u, 1u, 1u, 1u, 0u},
    {0u, 0u, 0u, 2u, 3u},
    {0u, 0u, 1u, 1u, 1u},
    {2u, 2u, 1u, 1u, 0u},
    {3u, 3u, 1u, 1u, 0u}
};

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

static inline __device__ uint32_t trade_get(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t base,
    uint32_t field
) {
    return state_get(states, stride, base + field, lane);
}

static inline __device__ void trade_set(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t base,
    uint32_t field,
    uint32_t value
) {
    state_set(states, stride, base + field, lane, value);
}

static inline __device__ void clear_trade(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t base
) {
    for (uint32_t field = 0u; field < TRADE_STRIDE; ++field) {
        state_set(states, stride, base + field, lane, 0u);
    }
}

static inline __device__ void copy_trade(
    uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t destination,
    uint32_t source
) {
    for (uint32_t field = 0u; field < TRADE_STRIDE; ++field) {
        state_set(
            states,
            stride,
            destination + field,
            lane,
            state_get(states, stride, source + field, lane)
        );
    }
}

static inline __device__ int trade_complete(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t base
) {
    if (trade_get(states, stride, lane, base, TRADE_PRESENT) == 0u) {
        return 0;
    }
    const uint32_t recipients = trade_get(states, stride, lane, base, TRADE_RECIPIENTS);
    const uint32_t responses = trade_get(states, stride, lane, base, TRADE_ACCEPTED)
        | trade_get(states, stride, lane, base, TRADE_REJECTED);
    return (responses & recipients) == recipients;
}

static inline __device__ uint32_t next_trade_recipient(
    uint32_t recipients,
    uint32_t start,
    uint32_t players
) {
    for (uint32_t player = start; player < players; ++player) {
        if ((recipients & (1u << player)) != 0u) {
            return player;
        }
    }
    return 0xffffffffu;
}

static inline __device__ uint32_t next_unanswered_trade_recipient(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t base,
    uint32_t current,
    uint32_t players
) {
    const uint32_t recipients = trade_get(states, stride, lane, base, TRADE_RECIPIENTS);
    const uint32_t responded = trade_get(states, stride, lane, base, TRADE_ACCEPTED)
        | trade_get(states, stride, lane, base, TRADE_REJECTED);
    for (uint32_t offset = 1u; offset <= players; ++offset) {
        const uint32_t player = (current + offset) % players;
        if ((recipients & (1u << player)) != 0u
            && (responded & (1u << player)) == 0u) {
            return player;
        }
    }
    return 0xffffffffu;
}

static inline __device__ uint32_t action_hand_total(
    const uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint32_t start
) {
    uint32_t total = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        total += action_get(actions, stride, start + resource, lane);
    }
    return total;
}

static inline __device__ int action_hands_disjoint(
    const uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint32_t give_start,
    uint32_t receive_start
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (action_get(actions, stride, give_start + resource, lane) > 0u
            && action_get(actions, stride, receive_start + resource, lane) > 0u) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ int player_contains_action_hand(
    const uint32_t *states,
    const uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t action_start
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (player_get(states, stride, lane, player, PLAYER_RESOURCES + resource)
            < action_get(actions, stride, action_start + resource, lane)) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ int player_contains_trade_hand(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t trade_base,
    uint32_t hand_offset
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (player_get(states, stride, lane, player, PLAYER_RESOURCES + resource)
            < trade_get(states, stride, lane, trade_base, hand_offset + resource)) {
            return 0;
        }
    }
    return 1;
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

static inline __device__ uint32_t longest_road_from_with_blocking_vertex(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t root_edge,
    uint32_t root_through,
    uint32_t blocking_vertex,
    uint32_t blocking_player
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
        const uint32_t owner = next_vertex == blocking_vertex
            ? blocking_player
            : building_player(building);
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

static inline __device__ uint32_t longest_road_from(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t root_edge,
    uint32_t root_through
) {
    return longest_road_from_with_blocking_vertex(
        states,
        topology,
        stride,
        lane,
        player,
        root_edge,
        root_through,
        0xffffffffu,
        0xffffffffu
    );
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

static inline __device__ uint32_t longest_road_length_with_blocking_vertex(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t blocking_vertex,
    uint32_t blocking_player
) {
    uint32_t best = 0u;
    for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
        if (state_get(states, stride, STATE_ROADS + edge, lane) != player + 1u) {
            continue;
        }
        const uint32_t a = topo_edge_vertex(topology, edge, 0u);
        const uint32_t b = topo_edge_vertex(topology, edge, 1u);
        const uint32_t from_a = longest_road_from_with_blocking_vertex(
            states,
            topology,
            stride,
            lane,
            player,
            edge,
            a,
            blocking_vertex,
            blocking_player
        );
        const uint32_t from_b = longest_road_from_with_blocking_vertex(
            states,
            topology,
            stride,
            lane,
            player,
            edge,
            b,
            blocking_vertex,
            blocking_player
        );
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

static inline __device__ int domestic_trade_allowed_for(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player
) {
    return state_get(states, stride, STATE_PLAYER_TRADES_ENABLED, lane) != 0u
        && (state_get(states, stride, STATE_DOMESTIC_TRADE_DISABLED, lane)
            & (1u << player)) == 0u;
}

static inline __device__ int domestic_trade_pair_allowed(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t left,
    uint32_t right
) {
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    if (left == right || left >= players || right >= players
        || !domestic_trade_allowed_for(states, stride, lane, left)
        || !domestic_trade_allowed_for(states, stride, lane, right)) {
        return 0;
    }
    const uint32_t embargoes = state_get(
        states, stride, STATE_DOMESTIC_TRADE_EMBARGOES, lane
    );
    const uint32_t left_to_right = 1u << (left * 4u + right);
    const uint32_t right_to_left = 1u << (right * 4u + left);
    return (embargoes & (left_to_right | right_to_left)) == 0u;
}

static inline __device__ uint32_t domestic_trade_recipients_for(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t creator
) {
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    uint32_t recipients = 0u;
    for (uint32_t recipient = 0u; recipient < players; ++recipient) {
        if (domestic_trade_pair_allowed(states, stride, lane, creator, recipient)) {
            recipients |= 1u << recipient;
        }
    }
    return recipients;
}

static inline __device__ int phase_is_chance(uint32_t phase) {
    return phase == PHASE_ROLL_CHANCE
        || phase == PHASE_DEVELOPMENT_CHANCE
        || phase == PHASE_RESOLVE_STEAL;
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
    if (phase == PHASE_TRADE_RESPONSES
        && trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) != 0u) {
        if (trade_complete(states, stride, lane, STATE_TRADE)) {
            return trade_get(states, stride, lane, STATE_TRADE, TRADE_CREATOR);
        }
        return state_get(states, stride, STATE_TRADE_CURSOR, lane);
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
        uint32_t affected_players = 0u;
        uint32_t sole_player = 0xffffffffu;
        for (uint32_t player = 0u; player < players; ++player) {
            const uint32_t owed = demand[player * 5u + resource];
            total += owed;
            if (owed > 0u) {
                affected_players += 1u;
                sole_player = player;
            }
        }
        const uint32_t bank = state_get(states, stride, STATE_BANK + resource, lane);
        if (total > bank) {
            if (affected_players == 1u && bank > 0u) {
                const uint32_t held = player_get(
                    states,
                    stride,
                    lane,
                    sole_player,
                    PLAYER_RESOURCES + resource
                );
                player_set(
                    states,
                    stride,
                    lane,
                    sole_player,
                    PLAYER_RESOURCES + resource,
                    held + bank
                );
                state_set(states, stride, STATE_BANK + resource, lane, 0u);
            }
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

static inline __device__ uint64_t mix_stream_seed(
    uint64_t base_seed,
    uint64_t global_index,
    uint64_t domain
) {
    uint64_t value = base_seed ^ domain ^ global_index * SEED_INDEX_MIX;
    value = (value ^ (value >> 30)) * 0xbf58476d1ce4e5b9ull;
    value = (value ^ (value >> 27)) * 0x94d049bb133111ebull;
    return value ^ (value >> 31);
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
    for (uint32_t field = 0u; field < ACTION_WORDS; ++field) {
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

static inline __device__ int weighted_reservoir_select(
    uint64_t *rng,
    uint32_t *total_weight,
    uint32_t weight
) {
    if (weight == 0u) {
        return 0;
    }
    const uint32_t next_total = *total_weight + weight;
    const int selected = rng_range(rng, next_total) < weight;
    *total_weight = next_total;
    return selected;
}

static inline __device__ void weighted_reservoir_action(
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint64_t *rng,
    uint32_t *total_weight,
    uint32_t weight,
    uint32_t tag,
    uint32_t arg0,
    uint32_t arg1,
    uint32_t arg2
) {
    if (weighted_reservoir_select(rng, total_weight, weight)) {
        write_action(actions, stride, lane, tag, arg0, arg1, arg2);
    }
}

static inline __device__ void write_offer_trade(
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    uint32_t recipients,
    const uint32_t give[5],
    const uint32_t receive[5]
) {
    actions[ACTION_TAG * stride + lane] = ACTION_OFFER_TRADE;
    actions[ACTION_ARG0 * stride + lane] = recipients;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        actions[(ACTION_ARG0 + 1u + resource) * stride + lane] = give[resource];
        actions[(ACTION_ARG0 + 6u + resource) * stride + lane] = receive[resource];
    }
}

static inline __device__ void write_counter_trade(
    uint32_t *actions,
    uint32_t stride,
    uint32_t lane,
    const uint32_t give[5],
    const uint32_t receive[5]
) {
    actions[ACTION_TAG * stride + lane] = ACTION_COUNTER_TRADE;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        actions[(ACTION_ARG0 + resource) * stride + lane] = give[resource];
        actions[(ACTION_ARG0 + 5u + resource) * stride + lane] = receive[resource];
    }
    actions[(ACTION_ARG0 + 10u) * stride + lane] = 0u;
}

static inline __device__ uint32_t resource_policy_score(uint32_t resource) {
    const uint32_t weights[5] = {100u, 100u, 78u, 125u, 115u};
    return resource < 5u ? weights[resource] : 1u;
}

static inline __device__ uint32_t trade_bundle_policy_value(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t base,
    uint32_t offset
) {
    uint32_t value = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        value += trade_get(states, stride, lane, base, offset + resource)
            * resource_policy_score(resource);
    }
    return value;
}

static inline __device__ uint32_t hand_total_fixed(const uint32_t hand[5]) {
    return hand[0] + hand[1] + hand[2] + hand[3] + hand[4];
}

static inline __device__ void copy_fixed_hand(
    uint32_t destination[5],
    const uint32_t source[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        destination[resource] = source[resource];
    }
}

static inline __device__ int fixed_hand_contains(
    const uint32_t hand[5],
    const uint32_t required[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (hand[resource] < required[resource]) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ int fixed_hands_disjoint(
    const uint32_t left[5],
    const uint32_t right[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (left[resource] > 0u && right[resource] > 0u) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ int fixed_hand_lex_less(
    const uint32_t left[5],
    const uint32_t right[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (left[resource] < right[resource]) {
            return 1;
        }
        if (left[resource] > right[resource]) {
            return 0;
        }
    }
    return 0;
}

static inline __device__ uint32_t encode_base4_hand(const uint32_t hand[5]) {
    uint32_t code = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        code = code * 4u + hand[resource];
    }
    return code;
}

static inline __device__ void decode_base4_hand(uint32_t code, uint32_t hand[5]) {
    for (int resource = 4; resource >= 0; --resource) {
        hand[resource] = code % 4u;
        code /= 4u;
    }
}

static inline __device__ uint32_t encode_base5_hand(const uint32_t hand[5]) {
    uint32_t code = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        code = code * 5u + hand[resource];
    }
    return code;
}

static inline __device__ void decode_base5_hand(uint32_t code, uint32_t hand[5]) {
    for (int resource = 4; resource >= 0; --resource) {
        hand[resource] = code % 5u;
        code /= 5u;
    }
}

static inline __device__ void append_domestic_request(
    uint16_t requests[32],
    uint32_t *count,
    const uint32_t request[5]
) {
    const uint32_t code = encode_base4_hand(request);
    for (uint32_t index = 0u; index < *count; ++index) {
        if (requests[index] == code) {
            return;
        }
    }
    if (*count < 32u) {
        requests[*count] = (uint16_t)code;
        *count += 1u;
    }
}

static inline __device__ uint32_t build_domestic_requests(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint16_t requests[32]
) {
    uint32_t count = 0u;
    for (uint32_t plan = 0u; plan < 6u; ++plan) {
        uint32_t missing[5];
        uint32_t missing_total = 0u;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t held = player_get(
                states, stride, lane, player, PLAYER_RESOURCES + resource
            );
            const uint32_t required = DOMESTIC_PLAN_COSTS[plan][resource];
            missing[resource] = required > held ? required - held : 0u;
            missing_total += missing[resource];
            for (uint32_t amount = 1u; amount <= missing[resource]; ++amount) {
                uint32_t request[5] = {0u, 0u, 0u, 0u, 0u};
                request[resource] = amount;
                append_domestic_request(requests, &count, request);
            }
        }
        for (uint32_t first = 0u; first < 5u; ++first) {
            if (missing[first] == 0u) {
                continue;
            }
            for (uint32_t second = first + 1u; second < 5u; ++second) {
                if (missing[second] == 0u) {
                    continue;
                }
                uint32_t request[5] = {0u, 0u, 0u, 0u, 0u};
                request[first] = 1u;
                request[second] = 1u;
                append_domestic_request(requests, &count, request);
            }
        }
        if (missing_total > 1u) {
            append_domestic_request(requests, &count, missing);
        }
    }
    if (resource_total(states, stride, lane, player)
        > state_get(states, stride, STATE_DISCARD_LIMIT, lane)) {
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            uint32_t request[5] = {0u, 0u, 0u, 0u, 0u};
            request[resource] = 1u;
            append_domestic_request(requests, &count, request);
        }
    }
    return count;
}

static inline __device__ void nearest_domestic_plan(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t nearest[5]
) {
    uint32_t best_missing = 0xffffffffu;
    for (uint32_t plan = 0u; plan < 6u; ++plan) {
        uint32_t missing = 0u;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t held = player_get(
                states, stride, lane, player, PLAYER_RESOURCES + resource
            );
            const uint32_t required = DOMESTIC_PLAN_COSTS[plan][resource];
            missing += required > held ? required - held : 0u;
        }
        if (missing < best_missing) {
            best_missing = missing;
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                nearest[resource] = DOMESTIC_PLAN_COSTS[plan][resource];
            }
        }
    }
}

static inline __device__ float domestic_give_opportunity_cost(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    const uint32_t nearest[5],
    const uint32_t give[5]
) {
    float score = 0.0f;
    uint32_t total = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t held = player_get(
            states, stride, lane, player, PLAYER_RESOURCES + resource
        );
        const uint32_t surplus = held > nearest[resource] ? held - nearest[resource] : 0u;
        const uint32_t cheap = give[resource] < surplus ? give[resource] : surplus;
        const uint32_t protected_count = give[resource] - cheap;
        score += (float)cheap * 0.08f + (float)protected_count;
        total += give[resource];
    }
    return score + (float)total * 0.01f;
}

static inline __device__ uint32_t build_mixed_domestic_bundles(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint16_t mixed_codes[48]
) {
    uint32_t nearest[5];
    nearest_domestic_plan(states, stride, lane, player, nearest);
    uint16_t frontier[126];
    uint32_t frontier_count = 1u;
    frontier[0] = 0u;
    uint32_t mixed_count = 0u;
    uint32_t expansions = 0u;

    while (frontier_count > 0u && expansions < 96u && mixed_count < 48u) {
        uint32_t best_index = 0u;
        uint32_t best_hand[5];
        decode_base5_hand((uint32_t)frontier[0], best_hand);
        float best_score = domestic_give_opportunity_cost(
            states, stride, lane, player, nearest, best_hand
        );
        for (uint32_t index = 1u; index < frontier_count; ++index) {
            uint32_t hand[5];
            decode_base5_hand((uint32_t)frontier[index], hand);
            const float score = domestic_give_opportunity_cost(
                states, stride, lane, player, nearest, hand
            );
            if (score < best_score
                || (score == best_score && frontier[index] < frontier[best_index])) {
                best_index = index;
                best_score = score;
                copy_fixed_hand(best_hand, hand);
            }
        }

        const uint16_t code = frontier[best_index];
        for (uint32_t index = best_index + 1u; index < frontier_count; ++index) {
            frontier[index - 1u] = frontier[index];
        }
        frontier_count -= 1u;
        expansions += 1u;

        uint32_t current[5];
        decode_base5_hand((uint32_t)code, current);
        const uint32_t total = hand_total_fixed(current);
        uint32_t kinds = 0u;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            kinds += current[resource] > 0u ? 1u : 0u;
        }
        if (total >= 2u && kinds >= 2u) {
            mixed_codes[mixed_count++] = code;
        }
        if (total >= 4u) {
            continue;
        }

        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t held = player_get(
                states, stride, lane, player, PLAYER_RESOURCES + resource
            );
            if (current[resource] >= held) {
                continue;
            }
            uint32_t next[5];
            copy_fixed_hand(next, current);
            next[resource] += 1u;
            const uint16_t next_code = (uint16_t)encode_base5_hand(next);
            int duplicate = 0;
            for (uint32_t index = 0u; index < frontier_count; ++index) {
                if (frontier[index] == next_code) {
                    duplicate = 1;
                    break;
                }
            }
            if (!duplicate) {
                for (uint32_t index = 0u; index < mixed_count; ++index) {
                    if (mixed_codes[index] == next_code) {
                        duplicate = 1;
                        break;
                    }
                }
            }
            if (!duplicate && frontier_count < 126u) {
                frontier[frontier_count++] = next_code;
            }
        }
    }
    return mixed_count;
}

static inline __device__ int decimal_debug_lex_less(
    uint32_t left,
    uint32_t right,
    uint32_t end_character
) {
    if (left == right) {
        return 0;
    }
    uint32_t left_power = 1u;
    uint32_t right_power = 1u;
    while (left / left_power >= 10u) {
        left_power *= 10u;
    }
    while (right / right_power >= 10u) {
        right_power *= 10u;
    }
    uint32_t left_rest = left;
    uint32_t right_rest = right;
    for (;;) {
        const uint32_t left_digit = left_rest / left_power;
        const uint32_t right_digit = right_rest / right_power;
        if (left_digit != right_digit) {
            return left_digit < right_digit;
        }
        left_rest %= left_power;
        right_rest %= right_power;
        const int left_done = left_power == 1u;
        const int right_done = right_power == 1u;
        if (left_done || right_done) {
            if (left_done && !right_done) {
                const uint32_t next_power = right_power / 10u;
                const uint32_t next_digit = right_rest / next_power;
                return end_character < (uint32_t)('0' + next_digit);
            }
            if (!left_done && right_done) {
                const uint32_t next_power = left_power / 10u;
                const uint32_t next_digit = left_rest / next_power;
                return (uint32_t)('0' + next_digit) < end_character;
            }
            return 0;
        }
        left_power /= 10u;
        right_power /= 10u;
    }
}

static inline __device__ int offer_debug_lex_less(
    const uint32_t give[5],
    const uint32_t receive[5],
    const uint32_t best_give[5],
    const uint32_t best_receive[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (give[resource] != best_give[resource]) {
            const uint32_t end_character = resource < 4u ? (uint32_t)',' : (uint32_t)']';
            return decimal_debug_lex_less(
                give[resource], best_give[resource], end_character
            );
        }
    }
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (receive[resource] != best_receive[resource]) {
            const uint32_t end_character = resource < 4u ? (uint32_t)',' : (uint32_t)']';
            return decimal_debug_lex_less(
                receive[resource], best_receive[resource], end_character
            );
        }
    }
    return 0;
}

static inline __device__ int rejected_trade_matches(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    const uint32_t give[5],
    const uint32_t receive[5]
) {
    if (trade_get(states, stride, lane, STATE_LAST_REJECTED_TRADE, TRADE_PRESENT) == 0u
        || trade_get(states, stride, lane, STATE_LAST_REJECTED_TRADE, TRADE_CREATOR)
            != state_get(states, stride, STATE_CURRENT_PLAYER, lane)) {
        return 0;
    }
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (trade_get(
                states, stride, lane, STATE_LAST_REJECTED_TRADE, TRADE_GIVE + resource
            ) != give[resource]
            || trade_get(
                states, stride, lane, STATE_LAST_REJECTED_TRADE, TRADE_RECEIVE + resource
            ) != receive[resource]) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ float domestic_offer_rules_score(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    const uint32_t give[5],
    const uint32_t receive[5]
) {
    uint32_t after[5];
    uint32_t give_total = 0u;
    uint32_t receive_total = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t held = player_get(
            states, stride, lane, player, PLAYER_RESOURCES + resource
        );
        after[resource] = held - give[resource] + receive[resource];
        give_total += give[resource];
        receive_total += receive[resource];
    }
    float completed = 0.0f;
    if (fixed_hand_contains(after, ROAD_COST)) {
        completed = fmaxf(completed, 1.2f);
    }
    if (fixed_hand_contains(after, SETTLEMENT_COST)) {
        completed = fmaxf(completed, 7.5f);
    }
    if (fixed_hand_contains(after, CITY_COST)) {
        completed = fmaxf(completed, 7.0f);
    }
    if (fixed_hand_contains(after, DEVELOPMENT_COST)) {
        completed = fmaxf(completed, 3.4f);
    }
    if (fixed_hand_contains(after, DOMESTIC_PLAN_COSTS[4])) {
        completed = fmaxf(completed, 8.8f);
    }
    if (fixed_hand_contains(after, DOMESTIC_PLAN_COSTS[5])) {
        completed = fmaxf(completed, 9.4f);
    }

    float nearest = 3.402823466e+38F;
    for (uint32_t plan = 0u; plan < 4u; ++plan) {
        uint32_t missing = 0u;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t required = DOMESTIC_PLAN_COSTS[plan][resource];
            missing += required > after[resource] ? required - after[resource] : 0u;
        }
        nearest = fminf(nearest, (float)missing);
    }
    float safety = 0.0f;
    if (resource_total(states, stride, lane, player)
        > state_get(states, stride, STATE_DISCARD_LIMIT, lane)
        && give_total > receive_total) {
        safety = (float)(give_total - receive_total) * 0.8f;
    }
    return completed
        + 1.5f / (1.0f + nearest)
        + (float)receive_total * 0.32f
        - (float)give_total * 0.18f
        + safety;
}

static inline __device__ void consider_best_domestic_offer(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    const uint32_t give[5],
    const uint32_t receive[5],
    int *found,
    float *best_score,
    uint32_t best_give[5],
    uint32_t best_receive[5]
) {
    if (!fixed_hands_disjoint(give, receive)
        || rejected_trade_matches(states, stride, lane, give, receive)) {
        return;
    }
    const uint32_t receive_total = hand_total_fixed(receive);
    uint32_t requested_resource = 0xffffffffu;
    if (receive_total == 1u) {
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            if (receive[resource] == 1u) {
                requested_resource = resource;
                break;
            }
        }
    }
    if (requested_resource != 0xffffffffu
        && state_get(states, stride, STATE_BANK_PUBLIC, lane) != 0u
        && state_get(states, stride, STATE_BANK + requested_resource, lane) > 0u) {
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            if (give[resource] >= trade_ratio(states, stride, lane, player, resource)) {
                return;
            }
        }
    }

    const float score = domestic_offer_rules_score(
        states, stride, lane, player, give, receive
    );
    if (!*found
        || score > *best_score
        || (score == *best_score
            && offer_debug_lex_less(give, receive, best_give, best_receive))) {
        *best_score = score;
        copy_fixed_hand(best_give, give);
        copy_fixed_hand(best_receive, receive);
        *found = 1;
    }
}

static inline __device__ int choose_best_domestic_trade_offer(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t selected_give[5],
    uint32_t selected_receive[5]
) {
    uint16_t requests[32];
    const uint32_t request_count = build_domestic_requests(
        states, stride, lane, player, requests
    );
    if (request_count == 0u) {
        return 0;
    }

    uint16_t mixed_codes[48];
    const uint32_t mixed_count = build_mixed_domestic_bundles(
        states, stride, lane, player, mixed_codes
    );

    int found = 0;
    float best_score = -3.402823466e+38F;
    for (uint32_t request_index = 0u; request_index < request_count; ++request_index) {
        uint32_t receive[5];
        decode_base4_hand((uint32_t)requests[request_index], receive);
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t held = player_get(
                states, stride, lane, player, PLAYER_RESOURCES + resource
            );
            for (uint32_t amount = 1u; amount <= held; ++amount) {
                uint32_t give[5] = {0u, 0u, 0u, 0u, 0u};
                give[resource] = amount;
                consider_best_domestic_offer(
                    states,
                    stride,
                    lane,
                    player,
                    give,
                    receive,
                    &found,
                    &best_score,
                    selected_give,
                    selected_receive
                );
            }
        }
        for (uint32_t mixed = 0u; mixed < mixed_count; ++mixed) {
            uint32_t give[5];
            decode_base5_hand((uint32_t)mixed_codes[mixed], give);
            consider_best_domestic_offer(
                states,
                stride,
                lane,
                player,
                give,
                receive,
                &found,
                &best_score,
                selected_give,
                selected_receive
            );
        }
    }
    return found;
}

static inline __device__ float counter_hand_score(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    const uint32_t hand[5]
) {
    float ready = 0.0f;
    if (fixed_hand_contains(hand, ROAD_COST)) {
        ready = fmaxf(ready, 0.28f);
    }
    if (fixed_hand_contains(hand, SETTLEMENT_COST)) {
        ready = fmaxf(ready, 1.42f);
    }
    if (fixed_hand_contains(hand, CITY_COST)) {
        ready = fmaxf(ready, 1.26f);
    }
    if (fixed_hand_contains(hand, DEVELOPMENT_COST)) {
        ready = fmaxf(ready, 0.72f);
    }

    float near = 0.0f;
    uint32_t missing = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        missing += ROAD_COST[resource] > hand[resource]
            ? ROAD_COST[resource] - hand[resource]
            : 0u;
    }
    near = fmaxf(near, 0.24f / (1.0f + (float)missing));
    missing = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        missing += SETTLEMENT_COST[resource] > hand[resource]
            ? SETTLEMENT_COST[resource] - hand[resource]
            : 0u;
    }
    near = fmaxf(near, 1.08f / (1.0f + (float)missing));
    missing = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        missing += CITY_COST[resource] > hand[resource]
            ? CITY_COST[resource] - hand[resource]
            : 0u;
    }
    near = fmaxf(near, 0.98f / (1.0f + (float)missing));
    missing = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        missing += DEVELOPMENT_COST[resource] > hand[resource]
            ? DEVELOPMENT_COST[resource] - hand[resource]
            : 0u;
    }
    near = fmaxf(near, 0.56f / (1.0f + (float)missing));

    const float weights[5] = {0.98f, 0.98f, 0.73f, 1.22f, 1.10f};
    float weighted = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        weighted += (float)hand[resource] * weights[resource];
    }
    weighted *= 0.11f;
    const uint32_t discard_limit = state_get(states, stride, STATE_DISCARD_LIMIT, lane);
    const uint32_t total = hand_total_fixed(hand);
    const float overflow = total > discard_limit ? (float)(total - discard_limit) : 0.0f;
    return ready + near + weighted - overflow * overflow * 0.045f;
}

static inline __device__ float counter_action_score(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t actor,
    uint32_t creator,
    const uint32_t give[5],
    const uint32_t receive[5],
    float before
) {
    uint32_t after[5];
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t held = player_get(
            states, stride, lane, actor, PLAYER_RESOURCES + resource
        );
        after[resource] = held - give[resource] + receive[resource];
    }
    const float creator_threat = (float)player_get(
        states, stride, lane, creator, PLAYER_PUBLIC_VP
    ) / (float)(state_get(states, stride, STATE_VICTORY_TARGET, lane) > 0u
        ? state_get(states, stride, STATE_VICTORY_TARGET, lane)
        : 1u);
    const float feeds_creator = (float)give[3] * 1.25f
        + (float)give[4] * 1.15f
        + (float)(give[0] + give[1]) * 0.68f;
    const float denies_creator = (float)receive[3] * 0.34f
        + (float)receive[4] * 0.30f;
    return counter_hand_score(states, stride, lane, after)
        - before
        - feeds_creator * creator_threat * 0.46f
        + denies_creator * creator_threat;
}

static inline __device__ int choose_best_counter_trade(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t selected_give[5],
    uint32_t selected_receive[5]
) {
    if (trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) == 0u
        || trade_complete(states, stride, lane, STATE_TRADE)
        || state_get(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane) >= 1u) {
        return 0;
    }
    const uint32_t actor = state_get(states, stride, STATE_TRADE_CURSOR, lane);
    const uint32_t creator = trade_get(states, stride, lane, STATE_TRADE, TRADE_CREATOR);
    if (actor >= state_get(states, stride, STATE_NUM_PLAYERS, lane)
        || creator >= state_get(states, stride, STATE_NUM_PLAYERS, lane)) {
        return 0;
    }

    uint32_t hand[5];
    uint32_t original_give[5];
    uint32_t original_receive[5];
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        hand[resource] = player_get(states, stride, lane, actor, PLAYER_RESOURCES + resource);
        original_give[resource] = trade_get(
            states, stride, lane, STATE_TRADE, TRADE_GIVE + resource
        );
        original_receive[resource] = trade_get(
            states, stride, lane, STATE_TRADE, TRADE_RECEIVE + resource
        );
    }

    uint32_t give_options[11][5];
    uint32_t receive_options[11][5];
    uint32_t give_count = 0u;
    uint32_t receive_count = 0u;
    copy_fixed_hand(give_options[give_count++], original_receive);
    copy_fixed_hand(receive_options[receive_count++], original_give);
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (hand[resource] > original_receive[resource]) {
            uint32_t option[5];
            copy_fixed_hand(option, original_receive);
            option[resource] += 1u;
            if (hand_total_fixed(option) <= 2u) {
                copy_fixed_hand(give_options[give_count++], option);
            }
        }
        uint32_t request[5];
        copy_fixed_hand(request, original_give);
        request[resource] += 1u;
        if (hand_total_fixed(request) <= 2u) {
            copy_fixed_hand(receive_options[receive_count++], request);
        }
        if (hand[resource] > 0u) {
            uint32_t one[5] = {0u, 0u, 0u, 0u, 0u};
            one[resource] = 1u;
            copy_fixed_hand(give_options[give_count++], one);
        }
        uint32_t one_request[5] = {0u, 0u, 0u, 0u, 0u};
        one_request[resource] = 1u;
        copy_fixed_hand(receive_options[receive_count++], one_request);
    }

    const float before = counter_hand_score(states, stride, lane, hand);
    float best_score = -3.402823466e+38F;
    int found = 0;
    for (uint32_t give_index = 0u; give_index < give_count; ++give_index) {
        const uint32_t *give = give_options[give_index];
        if (hand_total_fixed(give) == 0u || !fixed_hand_contains(hand, give)) {
            continue;
        }
        for (uint32_t receive_index = 0u; receive_index < receive_count; ++receive_index) {
            const uint32_t *receive = receive_options[receive_index];
            if (hand_total_fixed(receive) == 0u || !fixed_hands_disjoint(give, receive)) {
                continue;
            }
            int unchanged = 1;
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                if (give[resource] != original_receive[resource]
                    || receive[resource] != original_give[resource]) {
                    unchanged = 0;
                    break;
                }
            }
            if (unchanged) {
                continue;
            }
            const float score = counter_action_score(
                states, stride, lane, actor, creator, give, receive, before
            );
            int prefer = !found || score > best_score;
            if (found && score == best_score) {
                prefer = fixed_hand_lex_less(give, selected_give)
                    || (!fixed_hand_lex_less(selected_give, give)
                        && fixed_hand_lex_less(receive, selected_receive));
            }
            if (prefer) {
                best_score = score;
                copy_fixed_hand(selected_give, give);
                copy_fixed_hand(selected_receive, receive);
                found = 1;
            }
        }
    }
    return found;
}

static inline __device__ uint32_t pips_for_number(uint32_t number) {
    if (number == 2u || number == 12u) {
        return 1u;
    }
    if (number == 3u || number == 11u) {
        return 2u;
    }
    if (number == 4u || number == 10u) {
        return 3u;
    }
    if (number == 5u || number == 9u) {
        return 4u;
    }
    if (number == 6u || number == 8u) {
        return 5u;
    }
    return 0u;
}

#include "rollout_cutoff.cuh"

static inline __device__ uint32_t observed_monopoly_resource_weight(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t observer,
    uint32_t resource
) {
    uint32_t score = 1u;
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    for (uint32_t player = 0u; player < players; ++player) {
        if (player == observer) {
            continue;
        }
        uint32_t resource_pips = 0u;
        uint32_t total_pips = 0u;
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            const uint32_t building = state_get(states, stride, STATE_BUILDINGS + vertex, lane);
            if (building_player(building) != player) {
                continue;
            }
            const uint32_t multiplier = building_multiplier(building);
            const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
            for (uint32_t slot = 0u; slot < count; ++slot) {
                const uint32_t hex = topology[
                    TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot
                ];
                const uint32_t pips = pips_for_number(
                    state_get(states, stride, STATE_HEX_NUMBERS + hex, lane)
                ) * multiplier;
                total_pips += pips;
                const uint32_t encoded = state_get(
                    states, stride, STATE_HEX_RESOURCES + hex, lane
                );
                if (encoded == resource + 1u) {
                    resource_pips += pips;
                }
            }
        }
        // Exact opponent resource identities are private. Estimate the resource
        // share from public production and the public hand total instead.
        const uint32_t hand_total = resource_total(states, stride, lane, player);
        score += hand_total * (resource_pips + 1u) * 32u / (total_pips + 5u);
    }
    return score;
}

static inline __device__ uint32_t vertex_policy_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t vertex
) {
    uint32_t score = 50u;
    uint32_t resource_mask = 0u;
    const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
    for (uint32_t slot = 0u; slot < count; ++slot) {
        const uint32_t hex = topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot];
        score += pips_for_number(state_get(states, stride, STATE_HEX_NUMBERS + hex, lane)) * 24u;
        const uint32_t encoded = state_get(states, stride, STATE_HEX_RESOURCES + hex, lane);
        if (encoded > 0u) {
            resource_mask |= 1u << (encoded - 1u);
        }
    }
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if ((resource_mask & (1u << resource)) != 0u) {
            score += 18u;
        }
    }
    if (state_get(states, stride, STATE_PORTS + vertex, lane) != 0u) {
        score += 25u;
    }
    return score;
}

static inline __device__ int road_newly_enables_settlement_vertex(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge,
    uint32_t vertex
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
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t adjacent_edges = topo_vertex_edge_count(topology, vertex);
    int candidate_is_adjacent = 0;
    for (uint32_t slot = 0u; slot < adjacent_edges; ++slot) {
        const uint32_t adjacent = topo_vertex_edge(topology, vertex, slot);
        if (adjacent == edge) {
            candidate_is_adjacent = 1;
            continue;
        }
        if (state_get(states, stride, STATE_ROADS + adjacent, lane) == current + 1u) {
            return 0;
        }
    }
    return candidate_is_adjacent;
}

static inline __device__ uint32_t road_cut_settlement_policy_bonus(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge
) {
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    if (player_get(states, stride, lane, current, PLAYER_SETTLEMENTS_LEFT) == 0u) {
        return 0u;
    }
    int can_continue_directly = 1;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t held = player_get(
            states, stride, lane, current, PLAYER_RESOURCES + resource
        );
        if (held < ROAD_COST[resource] + SETTLEMENT_COST[resource]) {
            can_continue_directly = 0;
            break;
        }
    }

    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    const uint32_t old_holder = state_get(states, stride, STATE_LONGEST_HOLDER, lane);
    uint32_t best_bonus = 0u;
    for (uint32_t endpoint = 0u; endpoint < 2u; ++endpoint) {
        const uint32_t vertex = topo_edge_vertex(topology, edge, endpoint);
        if (!road_newly_enables_settlement_vertex(
            states, topology, stride, lane, edge, vertex
        )) {
            continue;
        }
        for (uint32_t opponent = 0u; opponent < players; ++opponent) {
            if (opponent == current) {
                continue;
            }
            const uint32_t before = longest_road_length(
                states, topology, stride, lane, opponent
            );
            const uint32_t after = longest_road_length_with_blocking_vertex(
                states, topology, stride, lane, opponent, vertex, current
            );
            const uint32_t road_loss = before > after ? before - after : 0u;
            if (road_loss == 0u) {
                continue;
            }

            int removes_award = 0;
            if (old_holder == opponent + 1u) {
                if (after < 5u) {
                    removes_award = 1;
                } else {
                    uint32_t best_other = 0u;
                    for (uint32_t player = 0u; player < players; ++player) {
                        if (player == opponent) {
                            continue;
                        }
                        const uint32_t other_length = player == current
                            ? longest_road_length(states, topology, stride, lane, player)
                            : longest_road_length_with_blocking_vertex(
                                states, topology, stride, lane, player, vertex, current
                            );
                        best_other = other_length > best_other ? other_length : best_other;
                    }
                    removes_award = best_other > after;
                }
            }
            if (road_loss < 2u && !removes_award) {
                continue;
            }

            uint32_t bonus = road_loss * ROAD_CUT_POLICY_ROAD_LOSS_WEIGHT;
            if (removes_award) {
                bonus += ROAD_CUT_POLICY_AWARD_LOSS_WEIGHT;
            }
            if (can_continue_directly) {
                bonus += bonus / 2u;
            }
            best_bonus = bonus > best_bonus ? bonus : best_bonus;
        }
    }
    return best_bonus;
}

static inline __device__ uint32_t road_policy_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge
) {
    uint32_t best = 50u;
    for (uint32_t endpoint = 0u; endpoint < 2u; ++endpoint) {
        const uint32_t vertex = topo_edge_vertex(topology, edge, endpoint);
        if (state_get(states, stride, STATE_BUILDINGS + vertex, lane) == 0u) {
            const uint32_t score = vertex_policy_score(states, topology, stride, lane, vertex);
            best = score > best ? score : best;
        }
        const uint32_t count = topo_vertex_vertex_count(topology, vertex);
        for (uint32_t slot = 0u; slot < count; ++slot) {
            const uint32_t next = topo_vertex_vertex(topology, vertex, slot);
            if (state_get(states, stride, STATE_BUILDINGS + next, lane) != 0u) {
                continue;
            }
            const uint32_t score = vertex_policy_score(states, topology, stride, lane, next) / 2u;
            best = score > best ? score : best;
        }
    }
    const uint32_t actor = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t before = cutoff_expansion(states, topology, stride, lane, actor, 0xffffffffu);
    const uint32_t after = cutoff_expansion(states, topology, stride, lane, actor, edge);
    const uint32_t gain = after > before ? after - before : 0;
    return best + gain * 3u + road_cut_settlement_policy_bonus(states, topology, stride, lane, edge);
}

static inline __device__ uint32_t profile_scaled_weight(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t profile_index,
    uint32_t base
) {
    uint32_t profile = player_get(states, stride, lane, player, PLAYER_POLICY_PROFILE + profile_index);
    profile = profile > 102u ? 102u : profile;
    return (base * (64u + profile)) / 115u;
}

static inline __device__ uint32_t maritime_policy_score(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t give,
    uint32_t receive,
    uint32_t ratio
) {
    uint32_t hand[5];
    for (uint32_t r = 0; r < 5; ++r) hand[r] = player_get(states, stride, lane, player, PLAYER_RESOURCES + r);
    uint32_t score = 0;
    // A maritime leg may receive a real deficit only after preserving every
    // card of the target. This excludes the ore->surplus grain->wool chain.
    for (uint32_t plan = 0; plan < 6; ++plan) {
        const uint32_t *cost = DOMESTIC_PLAN_COSTS[plan];
        if (hand[receive] >= cost[receive] || hand[give] < ratio + cost[give]) continue;
        if ((plan == 0 || plan >= 4) && player_get(states, stride, lane, player, PLAYER_ROADS_LEFT) == 0) continue;
        if ((plan == 1 || plan >= 4) && player_get(states, stride, lane, player, PLAYER_SETTLEMENTS_LEFT) == 0) continue;
        if (plan == 2 && player_get(states, stride, lane, player, PLAYER_CITIES_LEFT) == 0) continue;
        uint32_t remaining = 0;
        for (uint32_t r = 0; r < 5; ++r) {
            const uint32_t after = hand[r] - (r == give ? ratio : 0u) + (r == receive ? 1u : 0u);
            remaining += cost[r] > after ? cost[r] - after : 0;
        }
        const uint32_t importance = plan == 1 ? 900u : plan == 2 ? 850u : plan >= 4 ? 700u : plan == 3 ? 500u : 300u;
        const uint32_t candidate = importance / (1u + remaining);
        score = candidate > score ? candidate : score;
    }
    if (score == 0 && resource_total(states, stride, lane, player)
        > state_get(states, stride, STATE_DISCARD_LIMIT, lane)
        && hand[give] > 4 && hand[receive] < 2) score = 20;
    return score;
}

static inline __device__ uint32_t robber_policy_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t hex,
    uint32_t victim_code
) {
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    uint32_t opponent_pressure = 0u;
    uint32_t self_penalty = 0u;
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        const uint32_t building = state_get(states, stride, STATE_BUILDINGS + vertex, lane);
        const uint32_t owner = building_player(building);
        if (owner == 0xffffffffu) {
            continue;
        }
        const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
        int touches = 0;
        for (uint32_t slot = 0u; slot < count; ++slot) {
            if (topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot] == hex) {
                touches = 1;
                break;
            }
        }
        if (!touches) {
            continue;
        }
        const uint32_t production = pips_for_number(state_get(states, stride, STATE_HEX_NUMBERS + hex, lane))
            * building_multiplier(building);
        if (owner == current) {
            self_penalty += production * 45u;
        } else {
            opponent_pressure += production * (50u + player_get(states, stride, lane, owner, PLAYER_PUBLIC_VP) * 4u);
        }
    }
    uint32_t score = 100u + opponent_pressure;
    if (victim_code != 0u) {
        const uint32_t victim = victim_code - 1u;
        score += resource_total(states, stride, lane, victim) * 16u;
        score += player_get(states, stride, lane, victim, PLAYER_PUBLIC_VP) * 28u;
    }
    return score > self_penalty + 10u ? score - self_penalty : 10u;
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

static inline __device__ int can_afford_with_gains(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    const uint32_t cost[5],
    uint32_t first_gain,
    uint32_t second_gain
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        uint32_t held = player_get(states, stride, lane, player, PLAYER_RESOURCES + resource);
        held += first_gain == resource ? 1u : 0u;
        held += second_gain == resource ? 1u : 0u;
        if (held < cost[resource]) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ uint32_t immediate_build_completion_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t first_gain,
    uint32_t second_gain
) {
    const uint32_t public_vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP);
    const uint32_t hidden_vp = player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
    const uint32_t victory_target = state_get(states, stride, STATE_VICTORY_TARGET, lane);
    uint32_t score = 0u;

    if (player_get(states, stride, lane, player, PLAYER_CITIES_LEFT) > 0u
        && can_afford_with_gains(
            states, stride, lane, player, CITY_COST, first_gain, second_gain
        )) {
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            if (state_get(states, stride, STATE_BUILDINGS + vertex, lane) == player + 1u) {
                score = public_vp + hidden_vp + 1u >= victory_target ? 50000u : 12000u;
                break;
            }
        }
    }

    if (player_get(states, stride, lane, player, PLAYER_SETTLEMENTS_LEFT) > 0u
        && can_afford_with_gains(
            states, stride, lane, player, SETTLEMENT_COST, first_gain, second_gain
        )) {
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            if (can_place_settlement_device(states, topology, stride, lane, vertex, 0)) {
                const uint32_t settlement_score = public_vp + hidden_vp + 1u >= victory_target
                    ? 48000u
                    : 10000u;
                score = settlement_score > score ? settlement_score : score;
                break;
            }
        }
    }

    if (can_afford_with_gains(
        states, stride, lane, player, DEVELOPMENT_COST, first_gain, second_gain
    )) {
        score = score > 1800u ? score : 1800u;
    }
    if (player_get(states, stride, lane, player, PLAYER_ROADS_LEFT) > 0u
        && can_afford_with_gains(
            states, stride, lane, player, ROAD_COST, first_gain, second_gain
        )) {
        score = score > 900u ? score : 900u;
    }
    return score;
}

static inline __device__ uint32_t road_owner_with_pair(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t edge,
    uint32_t first,
    uint32_t second,
    uint32_t player
) {
    if (edge == first || edge == second) {
        return player + 1u;
    }
    return state_get(states, stride, STATE_ROADS + edge, lane);
}

static inline __device__ int can_place_settlement_with_pair(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t vertex,
    uint32_t first,
    uint32_t second
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
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t adjacent_edges = topo_vertex_edge_count(topology, vertex);
    for (uint32_t slot = 0u; slot < adjacent_edges; ++slot) {
        const uint32_t edge = topo_vertex_edge(topology, vertex, slot);
        if (road_owner_with_pair(states, stride, lane, edge, first, second, player) == player + 1u) {
            return 1;
        }
    }
    return 0;
}

static inline __device__ int actor_road_path_between(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t start,
    uint32_t goal
) {
    uint32_t stack[VERTEX_COUNT];
    unsigned char visited[VERTEX_COUNT];
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        visited[vertex] = 0u;
    }
    uint32_t size = 0u;
    stack[size++] = start;
    visited[start] = 1u;
    while (size > 0u) {
        const uint32_t vertex = stack[--size];
        if (vertex == goal) {
            return 1;
        }
        const uint32_t count = topo_vertex_edge_count(topology, vertex);
        for (uint32_t slot = 0u; slot < count; ++slot) {
            const uint32_t edge = topo_vertex_edge(topology, vertex, slot);
            if (state_get(states, stride, STATE_ROADS + edge, lane) != player + 1u) {
                continue;
            }
            const uint32_t a = topo_edge_vertex(topology, edge, 0u);
            const uint32_t b = topo_edge_vertex(topology, edge, 1u);
            const uint32_t next = a == vertex ? b : a;
            if (visited[next] == 0u) {
                visited[next] = 1u;
                stack[size++] = next;
            }
        }
    }
    return 0;
}

static inline __device__ uint32_t longest_road_from_with_pair(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t root_edge,
    uint32_t root_through,
    uint32_t first,
    uint32_t second
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
            if (road_owner_with_pair(
                states, stride, lane, candidate, first, second, player
            ) != player + 1u) {
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

static inline __device__ uint32_t longest_road_length_with_pair(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t first,
    uint32_t second
) {
    uint32_t best = 0u;
    for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
        if (road_owner_with_pair(states, stride, lane, edge, first, second, player) != player + 1u) {
            continue;
        }
        const uint32_t a = topo_edge_vertex(topology, edge, 0u);
        const uint32_t b = topo_edge_vertex(topology, edge, 1u);
        const uint32_t from_a = longest_road_from_with_pair(
            states, topology, stride, lane, player, edge, a, first, second
        );
        const uint32_t from_b = longest_road_from_with_pair(
            states, topology, stride, lane, player, edge, b, first, second
        );
        const uint32_t length = from_a > from_b ? from_a : from_b;
        best = length > best ? length : best;
    }
    return best;
}

static inline __device__ uint32_t road_building_pair_policy_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t first,
    uint32_t second
) {
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    uint32_t score = road_policy_score(states, topology, stride, lane, first);
    if (second < EDGE_COUNT) {
        score += road_policy_score(states, topology, stride, lane, second);
    }
    const uint32_t public_vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP);
    const uint32_t hidden_vp = player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
    const uint32_t victory_target = state_get(states, stride, STATE_VICTORY_TARGET, lane);

    if (player_get(states, stride, lane, player, PLAYER_SETTLEMENTS_LEFT) > 0u
        && has_cost(states, stride, lane, player, SETTLEMENT_COST)) {
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            if (!can_place_settlement_device(states, topology, stride, lane, vertex, 0)
                && can_place_settlement_with_pair(
                    states, topology, stride, lane, vertex, first, second
                )) {
                score += public_vp + hidden_vp + 1u >= victory_target ? 600000u : 160000u;
                break;
            }
        }
    }

    const uint32_t actor_length = longest_road_length_with_pair(
        states, topology, stride, lane, player, first, second
    );
    uint32_t best_other = 0u;
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    for (uint32_t opponent = 0u; opponent < players; ++opponent) {
        if (opponent == player) {
            continue;
        }
        const uint32_t length = longest_road_length(states, topology, stride, lane, opponent);
        best_other = length > best_other ? length : best_other;
    }
    const uint32_t old_holder = state_get(states, stride, STATE_LONGEST_HOLDER, lane);
    const int takes_longest = actor_length >= 5u
        && (old_holder == player + 1u || actor_length > best_other);
    if (takes_longest && old_holder != player + 1u) {
        score += public_vp + hidden_vp + 2u >= victory_target ? 700000u : 220000u;
    }

    const uint32_t first_a = topo_edge_vertex(topology, first, 0u);
    const uint32_t first_b = topo_edge_vertex(topology, first, 1u);
    int closes_existing_cycle = actor_road_path_between(
        states, topology, stride, lane, player, first_a, first_b
    );
    if (!closes_existing_cycle && second < EDGE_COUNT) {
        const uint32_t second_a = topo_edge_vertex(topology, second, 0u);
        const uint32_t second_b = topo_edge_vertex(topology, second, 1u);
        closes_existing_cycle = actor_road_path_between(
            states, topology, stride, lane, player, second_a, second_b
        );
    }
    if (closes_existing_cycle) {
        score += 120000u;
    }
    return score;
}

static inline __device__ int choose_road_building_pair(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint64_t *rng,
    uint32_t *selected_first,
    uint32_t *selected_second_code,
    uint32_t *selected_score
) {
    const uint32_t player = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t roads_left = player_get(states, stride, lane, player, PLAYER_ROADS_LEFT);
    uint32_t total_weight = 0u;
    *selected_first = 0xffffffffu;
    *selected_second_code = 0u;
    *selected_score = 0u;
    if (roads_left == 0u) {
        return 0;
    }
    if (roads_left == 1u) {
        for (uint32_t first = 0u; first < EDGE_COUNT; ++first) {
            if (!can_build_road_device(states, topology, stride, lane, first, 0xffffffffu)) {
                continue;
            }
            const uint32_t weight = road_building_pair_policy_score(
                states, topology, stride, lane, first, 0xffffffffu
            );
            const uint32_t next_total = total_weight + weight;
            if (rng_range(rng, next_total) < weight) {
                *selected_first = first;
                *selected_score = weight;
            }
            total_weight = next_total;
        }
        return *selected_first != 0xffffffffu;
    }
    for (uint32_t first = 0u; first < EDGE_COUNT; ++first) {
        if (!can_build_road_device(states, topology, stride, lane, first, 0xffffffffu)) {
            continue;
        }
        for (uint32_t second = 0u; second < EDGE_COUNT; ++second) {
            if (!can_build_road_device(states, topology, stride, lane, second, first)) {
                continue;
            }
            const uint32_t weight = road_building_pair_policy_score(
                states, topology, stride, lane, first, second
            );
            const uint32_t next_total = total_weight + weight;
            if (rng_range(rng, next_total) < weight) {
                *selected_first = first;
                *selected_second_code = second + 1u;
                *selected_score = weight;
            }
            total_weight = next_total;
        }
    }
    return *selected_first != 0xffffffffu;
}

static inline __device__ uint32_t year_of_plenty_pair_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t first,
    uint32_t second
) {
    return resource_policy_score(first)
        + resource_policy_score(second)
        + immediate_build_completion_score(
            states, topology, stride, lane, player, first, second
        );
}

static inline __device__ uint32_t monopoly_resource_score(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player,
    uint32_t resource
) {
    const uint32_t observed = observed_monopoly_resource_weight(
        states, topology, stride, lane, player, resource
    );
    const uint32_t single_gain_conversion = immediate_build_completion_score(
        states, topology, stride, lane, player, resource, 0xffffffffu
    );
    const uint32_t estimated_transfer = observed > 1u ? observed - 1u : 0u;
    const uint32_t conversion_scale = estimated_transfer < 32u ? estimated_transfer : 32u;
    return resource_policy_score(resource) * observed
        + single_gain_conversion * conversion_scale / 32u;
}

static inline __device__ uint32_t knight_policy_base(
    const uint32_t *states,
    uint32_t stride,
    uint32_t lane,
    uint32_t player
) {
    const uint32_t played = player_get(
        states, stride, lane, player, PLAYER_PLAYED_KNIGHTS
    ) + 1u;
    if (played < 3u) {
        return 1200u;
    }
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    for (uint32_t opponent = 0u; opponent < players; ++opponent) {
        if (opponent == player) {
            continue;
        }
        if (player_get(states, stride, lane, opponent, PLAYER_PLAYED_KNIGHTS) >= played) {
            return 1200u;
        }
    }
    if (state_get(states, stride, STATE_LARGEST_HOLDER, lane) == player + 1u) {
        return 1200u;
    }
    const uint32_t actor_vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP)
        + player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
    const uint32_t target = state_get(states, stride, STATE_VICTORY_TARGET, lane);
    return actor_vp + 2u >= target ? 20000000u : 7000u;
}

static inline __device__ int robber_blocks_actor_production(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint32_t player
) {
    const uint32_t robber_hex = state_get(states, stride, STATE_ROBBER_HEX, lane);
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        if (building_player(state_get(states, stride, STATE_BUILDINGS + vertex, lane)) != player) {
            continue;
        }
        const uint32_t count = topology[TOPO_VERTEX_HEX_COUNTS + vertex];
        for (uint32_t slot = 0u; slot < count; ++slot) {
            if (topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot] == robber_hex) {
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

static inline __device__ int choose_weighted_robber_action(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t stride,
    uint32_t lane,
    uint64_t *rng,
    uint32_t *selected_hex,
    uint32_t *selected_victim_code
) {
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    uint32_t total_weight = 0u;
    int found = 0;
    for (uint32_t hex = 0u; hex < HEX_COUNT; ++hex) {
        if (!robber_hex_allowed(states, topology, stride, lane, hex)) {
            continue;
        }
        const uint32_t victims = robber_victim_mask(states, topology, stride, lane, hex);
        if (victims == 0u) {
            const uint32_t weight = robber_policy_score(states, topology, stride, lane, hex, 0u);
            const uint32_t next_total = total_weight + weight;
            if (rng_range(rng, next_total) < weight) {
                *selected_hex = hex;
                *selected_victim_code = 0u;
            }
            total_weight = next_total;
            found = 1;
            continue;
        }
        for (uint32_t player = 0u; player < players; ++player) {
            if ((victims & (1u << player)) == 0u) {
                continue;
            }
            const uint32_t victim_code = player + 1u;
            const uint32_t weight = robber_policy_score(
                states, topology, stride, lane, hex, victim_code
            );
            const uint32_t next_total = total_weight + weight;
            if (rng_range(rng, next_total) < weight) {
                *selected_hex = hex;
                *selected_victim_code = victim_code;
            }
            total_weight = next_total;
            found = 1;
        }
    }
    return found;
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
    uint64_t *chance_rng_states,
    uint32_t stride,
    uint32_t lane
) {
    clear_action(actions, stride, lane);
    const uint32_t phase = state_get(states, stride, STATE_PHASE, lane);
    const int chance_phase = phase == PHASE_ROLL_CHANCE
        || phase == PHASE_DEVELOPMENT_CHANCE
        || phase == PHASE_RESOLVE_STEAL;
    uint64_t rng = chance_phase ? chance_rng_states[lane] : rng_states[lane];
    const uint32_t current = state_get(states, stride, STATE_CURRENT_PLAYER, lane);
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    uint32_t seen = 0u;

    if (phase == PHASE_SETUP_SETTLEMENT) {
        uint32_t total_weight = 0u;
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            if (can_place_settlement_device(states, topology, stride, lane, vertex, 1)) {
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &total_weight,
                    vertex_policy_score(states, topology, stride, lane, vertex),
                    ACTION_PLACE_SETTLEMENT,
                    vertex,
                    0u,
                    0u
                );
            }
        }
        seen = total_weight > 0u ? 1u : 0u;
    } else if (phase == PHASE_SETUP_ROAD) {
        const uint32_t settlement = state_get(states, stride, STATE_PHASE_ARG, lane);
        const uint32_t edge_count = topo_vertex_edge_count(topology, settlement);
        uint32_t total_weight = 0u;
        for (uint32_t slot = 0u; slot < edge_count; ++slot) {
            const uint32_t edge = topo_vertex_edge(topology, settlement, slot);
            if (state_get(states, stride, STATE_ROADS + edge, lane) == 0u) {
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &total_weight,
                    road_policy_score(states, topology, stride, lane, edge),
                    ACTION_PLACE_ROAD,
                    edge,
                    0u,
                    0u
                );
            }
        }
        seen = total_weight > 0u ? 1u : 0u;
    } else if (phase == PHASE_PRE_ROLL) {
        uint32_t total_weight = 0u;
        weighted_reservoir_action(
            actions,
            stride,
            lane,
            &rng,
            &total_weight,
            profile_scaled_weight(states, stride, lane, current, 0u, 6000u),
            ACTION_ROLL,
            0u,
            0u,
            0u
        );
        if (development_playable(states, stride, lane, current, 0u)) {
            uint32_t hex = 0u;
            uint32_t victim_code = 0u;
            if (choose_weighted_robber_action(
                states, topology, stride, lane, &rng, &hex, &victim_code
            )) {
                const uint32_t unblock_base = robber_blocks_actor_production(
                    states, topology, stride, lane, current
                ) ? 4200u : 1200u;
                const uint32_t decisive_base = knight_policy_base(
                    states, stride, lane, current
                );
                const uint32_t base = decisive_base > unblock_base
                    ? decisive_base
                    : unblock_base;
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &total_weight,
                    profile_scaled_weight(states, stride, lane, current, 2u, base),
                    ACTION_PLAY_KNIGHT,
                    hex,
                    victim_code,
                    0u
                );
            }
        }
        if (development_playable(states, stride, lane, current, 2u)
            && player_get(states, stride, lane, current, PLAYER_ROADS_LEFT) > 0u) {
            uint32_t first = 0xffffffffu;
            uint32_t second_code = 0u;
            uint32_t pair_score = 0u;
            if (choose_road_building_pair(
                states, topology, stride, lane, &rng, &first, &second_code, &pair_score
            )) {
                const uint32_t roads_left = player_get(
                    states, stride, lane, current, PLAYER_ROADS_LEFT
                );
                const uint32_t base = pair_score >= 10000u
                    ? 8000u
                    : (roads_left == 1u ? 24u : 1600u);
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &total_weight,
                    profile_scaled_weight(states, stride, lane, current, 2u, base),
                    ACTION_PLAY_ROAD_BUILDING,
                    first,
                    second_code,
                    0u
                );
            }
        }
        if (development_playable(states, stride, lane, current, 3u)) {
            uint32_t pair_weight = 0u;
            uint32_t selected_first = 0xffffffffu;
            uint32_t selected_second = 0xffffffffu;
            for (uint32_t first = 0u; first < 5u; ++first) {
                for (uint32_t second = first; second < 5u; ++second) {
                    const uint32_t needed = first == second ? 2u : 1u;
                    if (observed_bank_lower_bound(states, stride, lane, current, first) < needed
                        || observed_bank_lower_bound(states, stride, lane, current, second) == 0u) {
                        continue;
                    }
                    const uint32_t weight = year_of_plenty_pair_score(
                        states, topology, stride, lane, current, first, second
                    );
                    const uint32_t next_total = pair_weight + weight;
                    if (rng_range(&rng, next_total) < weight) {
                        selected_first = first;
                        selected_second = second;
                    }
                    pair_weight = next_total;
                }
            }
            if (selected_first != 0xffffffffu) {
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &total_weight,
                    profile_scaled_weight(
                        states,
                        stride,
                        lane,
                        current,
                        2u,
                        pair_weight >= 10000u ? 9000u : 3600u
                    ),
                    ACTION_PLAY_YEAR_OF_PLENTY,
                    selected_first,
                    selected_second,
                    0u
                );
            }
        }
        if (development_playable(states, stride, lane, current, 4u)) {
            uint32_t resource_weight = 0u;
            uint32_t selected_resource = 0u;
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                const uint32_t weight = monopoly_resource_score(
                    states, topology, stride, lane, current, resource
                );
                const uint32_t next_total = resource_weight + weight;
                if (rng_range(&rng, next_total) < weight) {
                    selected_resource = resource;
                }
                resource_weight = next_total;
            }
            weighted_reservoir_action(
                actions,
                stride,
                lane,
                &rng,
                &total_weight,
                profile_scaled_weight(
                    states,
                    stride,
                    lane,
                    current,
                    2u,
                    resource_weight >= 10000u ? 6000u : 1400u
                ),
                ACTION_PLAY_MONOPOLY,
                selected_resource,
                0u,
                0u
            );
        }
        seen = total_weight > 0u ? 1u : 0u;
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
        uint32_t hex = 0u;
        uint32_t victim_code = 0u;
        if (choose_weighted_robber_action(
            states, topology, stride, lane, &rng, &hex, &victim_code
        )) {
            write_action(actions, stride, lane, ACTION_MOVE_ROBBER, hex, victim_code, 0u);
            seen = 1u;
        }
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
        uint32_t family_weight = 0u;
        const uint32_t actor_vp = player_get(states, stride, lane, current, PLAYER_PUBLIC_VP)
            + player_get(states, stride, lane, current, PLAYER_DEVELOPMENT + 1u);
        const uint32_t victory_target = state_get(states, stride, STATE_VICTORY_TARGET, lane);

        weighted_reservoir_action(
            actions,
            stride,
            lane,
            &rng,
            &family_weight,
            profile_scaled_weight(states, stride, lane, current, 0u, 120u),
            ACTION_END_TURN,
            0u,
            0u,
            0u
        );

        if (player_get(states, stride, lane, current, PLAYER_ROADS_LEFT) > 0u
            && has_cost(states, stride, lane, current, ROAD_COST)) {
            uint32_t candidate_weight = 0u;
            uint32_t selected_edge = 0xffffffffu;
            for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
                if (!can_build_road_device(states, topology, stride, lane, edge, 0xffffffffu)) {
                    continue;
                }
                const uint32_t weight = road_policy_score(states, topology, stride, lane, edge);
                const uint32_t next_total = candidate_weight + weight;
                if (rng_range(&rng, next_total) < weight) {
                    selected_edge = edge;
                }
                candidate_weight = next_total;
            }
            if (selected_edge != 0xffffffffu) {
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(states, stride, lane, current, 1u, 900u),
                    ACTION_BUILD_ROAD,
                    selected_edge,
                    0u,
                    0u
                );
            }
        }

        if (player_get(states, stride, lane, current, PLAYER_SETTLEMENTS_LEFT) > 0u
            && has_cost(states, stride, lane, current, SETTLEMENT_COST)) {
            uint32_t candidate_weight = 0u;
            uint32_t selected_vertex = 0xffffffffu;
            for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
                if (!can_place_settlement_device(states, topology, stride, lane, vertex, 0)) {
                    continue;
                }
                const uint32_t weight = vertex_policy_score(states, topology, stride, lane, vertex);
                const uint32_t next_total = candidate_weight + weight;
                if (rng_range(&rng, next_total) < weight) {
                    selected_vertex = vertex;
                }
                candidate_weight = next_total;
            }
            if (selected_vertex != 0xffffffffu) {
                const uint32_t base = actor_vp + 1u >= victory_target ? 24000u : 3200u;
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(states, stride, lane, current, 1u, base),
                    ACTION_BUILD_SETTLEMENT,
                    selected_vertex,
                    0u,
                    0u
                );
            }
        }

        if (player_get(states, stride, lane, current, PLAYER_CITIES_LEFT) > 0u
            && has_cost(states, stride, lane, current, CITY_COST)) {
            uint32_t candidate_weight = 0u;
            uint32_t selected_vertex = 0xffffffffu;
            for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
                if (state_get(states, stride, STATE_BUILDINGS + vertex, lane) != current + 1u) {
                    continue;
                }
                const uint32_t weight = vertex_policy_score(states, topology, stride, lane, vertex) + 200u;
                const uint32_t next_total = candidate_weight + weight;
                if (rng_range(&rng, next_total) < weight) {
                    selected_vertex = vertex;
                }
                candidate_weight = next_total;
            }
            if (selected_vertex != 0xffffffffu) {
                const uint32_t base = actor_vp + 1u >= victory_target ? 26000u : 4200u;
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(states, stride, lane, current, 2u, base),
                    ACTION_BUILD_CITY,
                    selected_vertex,
                    0u,
                    0u
                );
            }
        }

        uint32_t deck_total = 0u;
        for (uint32_t card = 0u; card < 5u; ++card) {
            deck_total += state_get(states, stride, STATE_DEVELOPMENT_DECK + card, lane);
        }
        if (deck_total > 0u && has_cost(states, stride, lane, current, DEVELOPMENT_COST)) {
            weighted_reservoir_action(
                actions,
                stride,
                lane,
                &rng,
                &family_weight,
                profile_scaled_weight(states, stride, lane, current, 2u, 900u),
                ACTION_BUY_DEVELOPMENT,
                0u,
                0u,
                0u
            );
        }

        // Use guaranteed stock from the actor's hand and public hand totals
        // when the exact bank is hidden. Every compatible world has the same
        // proposed-action domain; unknown availability is not treated as known.
        {
            uint32_t maritime_weight = 0u;
            uint32_t maritime_give = 0xffffffffu;
            uint32_t maritime_receive = 0xffffffffu;
            uint32_t maritime_ratio = 0u;
            for (uint32_t give = 0u; give < 5u; ++give) {
                const uint32_t ratio = trade_ratio(states, stride, lane, current, give);
                if (player_get(states, stride, lane, current, PLAYER_RESOURCES + give) < ratio) {
                    continue;
                }
                for (uint32_t receive = 0u; receive < 5u; ++receive) {
                    if (give == receive || observed_bank_lower_bound(states, stride, lane, current, receive) == 0u) {
                        continue;
                    }
                    const uint32_t weight = maritime_policy_score(
                        states, stride, lane, current, give, receive, ratio
                    );
                    if (weight == 0u) continue;
                    const uint32_t next_total = maritime_weight + weight;
                    if (rng_range(&rng, next_total) < weight) {
                        maritime_give = give;
                        maritime_receive = receive;
                        maritime_ratio = ratio;
                    }
                    maritime_weight = next_total;
                }
            }
            if (maritime_give != 0xffffffffu) {
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(states, stride, lane, current, 0u, 700u),
                    ACTION_MARITIME_TRADE,
                    maritime_give,
                    maritime_receive,
                    maritime_ratio
                );
            }
        }


        if (development_playable(states, stride, lane, current, 0u)) {
            uint32_t hex = 0u;
            uint32_t victim_code = 0u;
            if (choose_weighted_robber_action(
                states, topology, stride, lane, &rng, &hex, &victim_code
            )) {
                const uint32_t base = knight_policy_base(states, stride, lane, current);
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(states, stride, lane, current, 2u, base),
                    ACTION_PLAY_KNIGHT,
                    hex,
                    victim_code,
                    0u
                );
            }
        }

        if (development_playable(states, stride, lane, current, 2u)
            && player_get(states, stride, lane, current, PLAYER_ROADS_LEFT) > 0u) {
            uint32_t first = 0xffffffffu;
            uint32_t second_code = 0u;
            uint32_t pair_score = 0u;
            if (choose_road_building_pair(
                states, topology, stride, lane, &rng, &first, &second_code, &pair_score
            )) {
                const uint32_t roads_left = player_get(
                    states, stride, lane, current, PLAYER_ROADS_LEFT
                );
                const uint32_t base = pair_score >= 10000u
                    ? 8000u
                    : (roads_left == 1u ? 24u : 1600u);
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(states, stride, lane, current, 2u, base),
                    ACTION_PLAY_ROAD_BUILDING,
                    first,
                    second_code,
                    0u
                );
            }
        }

        if (development_playable(states, stride, lane, current, 3u)) {
            uint32_t pair_weight = 0u;
            uint32_t selected_first = 0xffffffffu;
            uint32_t selected_second = 0xffffffffu;
            for (uint32_t first = 0u; first < 5u; ++first) {
                for (uint32_t second = first; second < 5u; ++second) {
                    const uint32_t needed = first == second ? 2u : 1u;
                    if (observed_bank_lower_bound(states, stride, lane, current, first) < needed
                        || observed_bank_lower_bound(states, stride, lane, current, second) == 0u) {
                        continue;
                    }
                    const uint32_t weight = year_of_plenty_pair_score(
                        states, topology, stride, lane, current, first, second
                    );
                    const uint32_t next_total = pair_weight + weight;
                    if (rng_range(&rng, next_total) < weight) {
                        selected_first = first;
                        selected_second = second;
                    }
                    pair_weight = next_total;
                }
            }
            if (selected_first != 0xffffffffu) {
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &family_weight,
                    profile_scaled_weight(
                        states,
                        stride,
                        lane,
                        current,
                        2u,
                        pair_weight >= 10000u ? 9000u : 3600u
                    ),
                    ACTION_PLAY_YEAR_OF_PLENTY,
                    selected_first,
                    selected_second,
                    0u
                );
            }
        }

        if (development_playable(states, stride, lane, current, 4u)) {
            uint32_t resource_weight = 0u;
            uint32_t selected_resource = 0u;
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                const uint32_t weight = monopoly_resource_score(
                    states, topology, stride, lane, current, resource
                );
                const uint32_t next_total = resource_weight + weight;
                if (rng_range(&rng, next_total) < weight) {
                    selected_resource = resource;
                }
                resource_weight = next_total;
            }
            weighted_reservoir_action(
                actions,
                stride,
                lane,
                &rng,
                &family_weight,
                profile_scaled_weight(
                    states,
                    stride,
                    lane,
                    current,
                    2u,
                    resource_weight >= 10000u ? 6000u : 1400u
                ),
                ACTION_PLAY_MONOPOLY,
                selected_resource,
                0u,
                0u
            );
        }

        // Domestic offers are expensive to rank because the CPU rules contract
        // includes mixed and multi-card bundles. Keep the complete fidelity
        // search lazy: first let the domestic family compete using a cheap
        // build-readiness weight, and only materialize/rank the top CPU-admitted
        // offer when that family actually wins the weighted draw.
        const uint32_t domestic_recipients = domestic_trade_recipients_for(
            states, stride, lane, current
        );
        if (domestic_recipients != 0u
            && state_get(states, stride, STATE_DOMESTIC_TRADE_COUNT, lane) < 2u
            && resource_total(states, stride, lane, current) > 0u) {
            uint32_t nearest_missing = 0xffffffffu;
            for (uint32_t plan = 0u; plan < 6u; ++plan) {
                uint32_t missing = 0u;
                for (uint32_t resource = 0u; resource < 5u; ++resource) {
                    const uint32_t held = player_get(
                        states, stride, lane, current, PLAYER_RESOURCES + resource
                    );
                    const uint32_t required = DOMESTIC_PLAN_COSTS[plan][resource];
                    missing += required > held ? required - held : 0u;
                }
                nearest_missing = missing < nearest_missing ? missing : nearest_missing;
            }
            const uint32_t hand_total = resource_total(states, stride, lane, current);
            const uint32_t overflow = hand_total
                > state_get(states, stride, STATE_DISCARD_LIMIT, lane)
                ? hand_total - state_get(states, stride, STATE_DISCARD_LIMIT, lane)
                : 0u;
            const uint32_t base = 320u
                + 720u / (1u + nearest_missing)
                + (overflow < 8u ? overflow : 8u) * 70u;
            const uint32_t weight = profile_scaled_weight(
                states, stride, lane, current, 3u, base
            );
            if (weighted_reservoir_select(&rng, &family_weight, weight)) {
                uint32_t give[5];
                uint32_t receive[5];
                if (choose_best_domestic_trade_offer(
                    states, stride, lane, current, give, receive
                )) {
                    write_offer_trade(
                        actions, stride, lane, domestic_recipients, give, receive
                    );
                }
            }
        }
        seen = family_weight > 0u ? 1u : 0u;
    } else if (phase == PHASE_TRADE_RESPONSES) {
        if (trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) != 0u) {
            uint32_t total_weight = 0u;
            if (trade_complete(states, stride, lane, STATE_TRADE)) {
                const uint32_t creator = trade_get(
                    states, stride, lane, STATE_TRADE, TRADE_CREATOR
                );
                weighted_reservoir_action(
                    actions,
                    stride,
                    lane,
                    &rng,
                    &total_weight,
                    profile_scaled_weight(states, stride, lane, creator, 0u, 120u),
                    ACTION_CANCEL_TRADE,
                    0u,
                    0u,
                    0u
                );
                if (creator < players
                    && domestic_trade_allowed_for(states, stride, lane, creator)) {
                    const uint32_t accepted = trade_get(
                        states, stride, lane, STATE_TRADE, TRADE_ACCEPTED
                    );
                    const uint32_t give_value = trade_bundle_policy_value(
                        states, stride, lane, STATE_TRADE, TRADE_GIVE
                    );
                    const uint32_t receive_value = trade_bundle_policy_value(
                        states, stride, lane, STATE_TRADE, TRADE_RECEIVE
                    );
                    const uint32_t benefit = receive_value > give_value
                        ? receive_value - give_value
                        : 0u;
                    for (uint32_t partner = 0u; partner < players; ++partner) {
                        if ((accepted & (1u << partner)) == 0u
                            || !domestic_trade_pair_allowed(
                                states, stride, lane, creator, partner
                            )
                            || !player_contains_trade_hand(
                                states, stride, lane, creator, STATE_TRADE, TRADE_GIVE
                            )
                            || !player_contains_trade_hand(
                                states, stride, lane, partner, STATE_TRADE, TRADE_RECEIVE
                            )) {
                            continue;
                        }
                        weighted_reservoir_action(
                            actions,
                            stride,
                            lane,
                            &rng,
                            &total_weight,
                            profile_scaled_weight(
                                states, stride, lane, creator, 3u, 900u + benefit * 4u
                            ),
                            ACTION_CONFIRM_TRADE,
                            partner,
                            0u,
                            0u
                        );
                    }
                }
            } else {
                const uint32_t actor = state_get(
                    states, stride, STATE_TRADE_CURSOR, lane
                );
                if (actor < players) {
                    weighted_reservoir_action(
                        actions,
                        stride,
                        lane,
                        &rng,
                        &total_weight,
                        profile_scaled_weight(states, stride, lane, actor, 4u, 700u),
                        ACTION_RESPOND_TRADE,
                        0u,
                        0u,
                        0u
                    );
                    const uint32_t creator = trade_get(
                        states, stride, lane, STATE_TRADE, TRADE_CREATOR
                    );
                    if (domestic_trade_pair_allowed(states, stride, lane, creator, actor)
                        && player_contains_trade_hand(
                            states, stride, lane, actor, STATE_TRADE, TRADE_RECEIVE
                        )) {
                        const uint32_t incoming = trade_bundle_policy_value(
                            states, stride, lane, STATE_TRADE, TRADE_GIVE
                        );
                        const uint32_t outgoing = trade_bundle_policy_value(
                            states, stride, lane, STATE_TRADE, TRADE_RECEIVE
                        );
                        const uint32_t benefit = incoming > outgoing
                            ? incoming - outgoing
                            : 0u;
                        weighted_reservoir_action(
                            actions,
                            stride,
                            lane,
                            &rng,
                            &total_weight,
                            profile_scaled_weight(
                                states, stride, lane, actor, 3u, 520u + benefit * 5u
                            ),
                            ACTION_RESPOND_TRADE,
                            1u,
                            0u,
                            0u
                        );
                    }
                    if (domestic_trade_pair_allowed(states, stride, lane, creator, actor)
                        && state_get(
                            states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane
                        ) < 1u) {
                        uint32_t give[5];
                        uint32_t receive[5];
                        if (choose_best_counter_trade(
                            states, stride, lane, give, receive
                        )) {
                            const uint32_t weight = profile_scaled_weight(
                                states, stride, lane, actor, 3u, 260u
                            );
                            if (weighted_reservoir_select(
                                &rng, &total_weight, weight
                            )) {
                                write_counter_trade(
                                    actions, stride, lane, give, receive
                                );
                            }
                        }
                    }
                }
            }
            seen = total_weight > 0u ? 1u : 0u;
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

    // A valid standard-rules state should always expose a rollout action.
    // Leave tag 254 as a detectable sentinel instead of fabricating a transition.
    if (seen == 0u) {
        write_action(actions, stride, lane, 254u, 0u, 0u, 0u);
    }
    if (chance_phase) {
        chance_rng_states[lane] = rng;
    } else {
        rng_states[lane] = rng;
    }
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
        if (phase != PHASE_PRE_ROLL && phase != PHASE_MAIN) {
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
        if (phase != PHASE_PRE_ROLL && phase != PHASE_MAIN) {
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
        if (phase != PHASE_PRE_ROLL && phase != PHASE_MAIN) {
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

    if (tag == ACTION_OFFER_TRADE) {
        const uint32_t recipients = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t give_start = ACTION_ARG0 + 1u;
        const uint32_t receive_start = ACTION_ARG0 + 6u;
        const uint32_t allowed_recipients = domestic_trade_recipients_for(
            states, stride, lane, current
        );
        if (!domestic_trade_allowed_for(states, stride, lane, current)
            || phase != PHASE_MAIN
            || recipients == 0u
            || (recipients & ~allowed_recipients) != 0u
            || (recipients & (1u << current)) != 0u
            || (recipients >> players) != 0u
            || action_hand_total(actions, stride, lane, give_start) == 0u
            || action_hand_total(actions, stride, lane, receive_start) == 0u
            || !player_contains_action_hand(
                states, actions, stride, lane, current, give_start
            )
            || !action_hands_disjoint(
                actions, stride, lane, give_start, receive_start
            )) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t first = next_trade_recipient(recipients, 0u, players);
        if (first == 0xffffffffu) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        clear_trade(states, stride, lane, STATE_TRADE);
        trade_set(states, stride, lane, STATE_TRADE, TRADE_PRESENT, 1u);
        trade_set(states, stride, lane, STATE_TRADE, TRADE_CREATOR, current);
        trade_set(states, stride, lane, STATE_TRADE, TRADE_RECIPIENTS, recipients);
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            trade_set(
                states,
                stride,
                lane,
                STATE_TRADE,
                TRADE_GIVE + resource,
                action_get(actions, stride, give_start + resource, lane)
            );
            trade_set(
                states,
                stride,
                lane,
                STATE_TRADE,
                TRADE_RECEIVE + resource,
                action_get(actions, stride, receive_start + resource, lane)
            );
        }
        state_set(states, stride, STATE_DOMESTIC_TRADE_USED, lane, 1u);
        const uint32_t count = state_get(states, stride, STATE_DOMESTIC_TRADE_COUNT, lane);
        state_set(
            states,
            stride,
            STATE_DOMESTIC_TRADE_COUNT,
            lane,
            count < 255u ? count + 1u : 255u
        );
        state_set(states, stride, STATE_TRADE_CURSOR, lane, first);
        state_set(states, stride, STATE_PHASE, lane, PHASE_TRADE_RESPONSES);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        state_set(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane, 0u);
        return;
    }

    if (tag == ACTION_RESPOND_TRADE) {
        const uint32_t accept = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t cursor = state_get(states, stride, STATE_TRADE_CURSOR, lane);
        if (phase != PHASE_TRADE_RESPONSES
            || trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) == 0u
            || trade_complete(states, stride, lane, STATE_TRADE)) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t creator = trade_get(
            states, stride, lane, STATE_TRADE, TRADE_CREATOR
        );
        if (accept > 1u
            || (accept != 0u
                && !domestic_trade_pair_allowed(
                    states, stride, lane, creator, cursor
                ))) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t recipients = trade_get(
            states, stride, lane, STATE_TRADE, TRADE_RECIPIENTS
        );
        if (cursor >= players || (recipients & (1u << cursor)) == 0u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        if (accept != 0u
            && !player_contains_trade_hand(
                states, stride, lane, cursor, STATE_TRADE, TRADE_RECEIVE
            )) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        const uint32_t response_field = accept != 0u ? TRADE_ACCEPTED : TRADE_REJECTED;
        trade_set(
            states,
            stride,
            lane,
            STATE_TRADE,
            response_field,
            trade_get(states, stride, lane, STATE_TRADE, response_field) | (1u << cursor)
        );
        if (!trade_complete(states, stride, lane, STATE_TRADE)) {
            const uint32_t next = next_unanswered_trade_recipient(
                states, stride, lane, STATE_TRADE, cursor, players
            );
            if (next == 0xffffffffu) {
                status[lane] = STATUS_INVALID_ACTION;
                return;
            }
            state_set(states, stride, STATE_TRADE_CURSOR, lane, next);
        }
        return;
    }

    if (tag == ACTION_COUNTER_TRADE) {
        const uint32_t give_start = ACTION_ARG0;
        const uint32_t receive_start = ACTION_ARG0 + 5u;
        const uint32_t actor = state_get(states, stride, STATE_TRADE_CURSOR, lane);
        const uint32_t previous_creator = trade_get(
            states, stride, lane, STATE_TRADE, TRADE_CREATOR
        );
        if (!domestic_trade_pair_allowed(
                states, stride, lane, previous_creator, actor
            )
            || phase != PHASE_TRADE_RESPONSES
            || state_get(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane) >= 1u
            || trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) == 0u
            || trade_complete(states, stride, lane, STATE_TRADE)) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        if (actor >= players
            || action_hand_total(actions, stride, lane, give_start) == 0u
            || action_hand_total(actions, stride, lane, receive_start) == 0u
            || !player_contains_action_hand(
                states, actions, stride, lane, actor, give_start
            )
            || !action_hands_disjoint(
                actions, stride, lane, give_start, receive_start
            )) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        clear_trade(states, stride, lane, STATE_TRADE);
        trade_set(states, stride, lane, STATE_TRADE, TRADE_PRESENT, 1u);
        trade_set(states, stride, lane, STATE_TRADE, TRADE_CREATOR, actor);
        const uint32_t recipients = domestic_trade_recipients_for(
            states, stride, lane, actor
        );
        if (recipients == 0u) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        trade_set(
            states,
            stride,
            lane,
            STATE_TRADE,
            TRADE_RECIPIENTS,
            recipients
        );
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            trade_set(
                states,
                stride,
                lane,
                STATE_TRADE,
                TRADE_GIVE + resource,
                action_get(actions, stride, give_start + resource, lane)
            );
            trade_set(
                states,
                stride,
                lane,
                STATE_TRADE,
                TRADE_RECEIVE + resource,
                action_get(actions, stride, receive_start + resource, lane)
            );
        }
        state_set(states, stride, STATE_TRADE_CURSOR, lane, previous_creator);
        state_set(
            states,
            stride,
            STATE_TRADE_NEGOTIATION_ROUND,
            lane,
            state_get(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane) + 1u
        );
        return;
    }

    if (tag == ACTION_CONFIRM_TRADE) {
        const uint32_t partner = action_get(actions, stride, ACTION_ARG0, lane);
        const uint32_t creator = trade_get(states, stride, lane, STATE_TRADE, TRADE_CREATOR);
        if (!domestic_trade_allowed_for(states, stride, lane, creator)
            || phase != PHASE_TRADE_RESPONSES
            || trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) == 0u
            || !trade_complete(states, stride, lane, STATE_TRADE)) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        const uint32_t accepted = trade_get(states, stride, lane, STATE_TRADE, TRADE_ACCEPTED);
        if (creator >= players
            || partner >= players
            || !domestic_trade_pair_allowed(states, stride, lane, creator, partner)
            || (accepted & (1u << partner)) == 0u
            || !player_contains_trade_hand(
                states, stride, lane, creator, STATE_TRADE, TRADE_GIVE
            )
            || !player_contains_trade_hand(
                states, stride, lane, partner, STATE_TRADE, TRADE_RECEIVE
            )) {
            status[lane] = STATUS_INVALID_ACTION;
            return;
        }
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const uint32_t give = trade_get(
                states, stride, lane, STATE_TRADE, TRADE_GIVE + resource
            );
            const uint32_t receive = trade_get(
                states, stride, lane, STATE_TRADE, TRADE_RECEIVE + resource
            );
            player_set(
                states,
                stride,
                lane,
                creator,
                PLAYER_RESOURCES + resource,
                player_get(states, stride, lane, creator, PLAYER_RESOURCES + resource)
                    - give
                    + receive
            );
            player_set(
                states,
                stride,
                lane,
                partner,
                PLAYER_RESOURCES + resource,
                player_get(states, stride, lane, partner, PLAYER_RESOURCES + resource)
                    - receive
                    + give
            );
        }
        clear_trade(states, stride, lane, STATE_TRADE);
        clear_trade(states, stride, lane, STATE_LAST_REJECTED_TRADE);
        state_set(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane, 0u);
        state_set(states, stride, STATE_PHASE, lane, PHASE_MAIN);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        return;
    }

    if (tag == ACTION_CANCEL_TRADE) {
        if (phase != PHASE_TRADE_RESPONSES
            || trade_get(states, stride, lane, STATE_TRADE, TRADE_PRESENT) == 0u
            || !trade_complete(states, stride, lane, STATE_TRADE)) {
            status[lane] = STATUS_INVALID_PHASE;
            return;
        }
        copy_trade(
            states, stride, lane, STATE_LAST_REJECTED_TRADE, STATE_TRADE
        );
        clear_trade(states, stride, lane, STATE_TRADE);
        state_set(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane, 0u);
        state_set(states, stride, STATE_PHASE, lane, PHASE_MAIN);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
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
        clear_trade(states, stride, lane, STATE_LAST_REJECTED_TRADE);
        state_set(states, stride, STATE_TRADE_NEGOTIATION_ROUND, lane, 0u);
        state_set(states, stride, STATE_CURRENT_PLAYER, lane, (current + 1u) % players);
        state_set(states, stride, STATE_TURN, lane, state_get(states, stride, STATE_TURN, lane) + 1u);
        state_set(states, stride, STATE_LAST_ROLL, lane, 0u);
        state_set(states, stride, STATE_PHASE, lane, PHASE_PRE_ROLL);
        state_set(states, stride, STATE_PHASE_ARG, lane, 0u);
        finish_if_won(states, stride, lane);
        return;
    }

    status[lane] = STATUS_UNSUPPORTED_ACTION;
}

extern "C" __global__ void generate_rollout_actions_batch_kernel(
    const uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint64_t *rng_states,
    uint64_t *chance_rng_states,
    uint32_t stride,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    generate_rollout_action_lane(
        states, topology, actions, rng_states, chance_rng_states, stride, lane
    );
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
    uint64_t *chance_rng_states,
    uint32_t stride,
    uint32_t count,
    uint32_t steps
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    for (uint32_t step = 0u; step < steps && status[lane] == STATUS_OK; ++step) {
        generate_rollout_action_lane(
            states, topology, actions, rng_states, chance_rng_states, stride, lane
        );
        apply_transition_lane(states, topology, actions, status, stride, lane);
        if (state_get(states, stride, STATE_PHASE, lane) == PHASE_FINISHED) {
            break;
        }
    }
}

extern "C" __global__ void assign_rotating_profiles_kernel(
    uint32_t *states,
    const uint32_t *profiles,
    uint64_t game_offset,
    uint32_t stride,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
    if (players == 0u || players > MAX_PLAYERS) {
        return;
    }
    const uint32_t candidate = (uint32_t)((game_offset + (uint64_t)lane) % (uint64_t)players);
    for (uint32_t player = 0u; player < players; ++player) {
        const uint32_t profile_base = player == candidate ? 0u : 5u;
        for (uint32_t index = 0u; index < 5u; ++index) {
            player_set(
                states,
                stride,
                lane,
                player,
                PLAYER_POLICY_PROFILE + index,
                profiles[profile_base + index]
            );
        }
    }
}

extern "C" __global__ void run_until_candidate_kernel(
    uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint32_t *status,
    uint64_t *rng_states,
    uint64_t *chance_rng_states,
    uint32_t *action_counts,
    uint32_t *candidate_ready,
    uint64_t game_offset,
    uint32_t stride,
    uint32_t count,
    uint32_t max_actions,
    uint32_t max_turns
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    candidate_ready[lane] = 0u;
    uint32_t executed = action_counts[lane];
    while (executed < max_actions && status[lane] == STATUS_OK) {
        const uint32_t phase = state_get(states, stride, STATE_PHASE, lane);
        if (phase == PHASE_FINISHED
            || state_get(states, stride, STATE_TURN, lane) >= max_turns) {
            break;
        }
        const uint32_t players = state_get(states, stride, STATE_NUM_PLAYERS, lane);
        if (players == 0u || players > MAX_PLAYERS) {
            status[lane] = STATUS_INVALID_STATE;
            break;
        }
        const uint32_t candidate = (uint32_t)(
            (game_offset + (uint64_t)lane) % (uint64_t)players
        );
        if (!phase_is_chance(phase) && state_actor(states, stride, lane) == candidate) {
            candidate_ready[lane] = 1u;
            break;
        }
        generate_rollout_action_lane(
            states, topology, actions, rng_states, chance_rng_states, stride, lane
        );
        apply_transition_lane(states, topology, actions, status, stride, lane);
        if (status[lane] == STATUS_OK) {
            executed += 1u;
        }
    }
    action_counts[lane] = executed;
}

extern "C" __global__ void apply_candidate_actions_kernel(
    uint32_t *states,
    const uint32_t *topology,
    const uint32_t *actions,
    uint32_t *status,
    uint32_t *action_counts,
    const uint32_t *candidate_ready,
    uint32_t stride,
    uint32_t count
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count || candidate_ready[lane] == 0u || status[lane] != STATUS_OK) {
        return;
    }
    apply_transition_lane(states, topology, actions, status, stride, lane);
    if (status[lane] == STATUS_OK) {
        action_counts[lane] += 1u;
    }
}

extern "C" __global__ void sample_candidate_root_actions_kernel(
    const uint32_t *base_states,
    uint32_t base_stride,
    const uint32_t *topology,
    const uint32_t *candidate_ready,
    uint32_t *root_states,
    uint32_t *root_actions,
    uint64_t *root_rng_states,
    uint64_t *root_chance_rng_states,
    uint32_t *root_base_indices,
    uint64_t *root_seed_keys,
    uint64_t game_offset,
    uint64_t seed,
    uint32_t game_count,
    uint32_t roots_per_game,
    uint32_t root_count
) {
    const uint32_t root = blockIdx.x * blockDim.x + threadIdx.x;
    if (root >= root_count || roots_per_game == 0u) {
        return;
    }
    const uint32_t game = root / roots_per_game;
    if (game >= game_count) {
        return;
    }
    root_base_indices[root] = game;
    const uint32_t slot = root % roots_per_game;
    const uint64_t global_game = game_offset + (uint64_t)game;
    const uint64_t proposal_index = global_game * (uint64_t)roots_per_game + (uint64_t)slot;
    root_seed_keys[root] = proposal_index;
    if (candidate_ready[game] == 0u) {
        for (uint32_t field = 0u; field < ACTION_WORDS; ++field) {
            root_actions[field * root_count + root] = 0u;
        }
        root_actions[ACTION_TAG * root_count + root] = 254u;
        return;
    }
    for (uint32_t field = 0u; field < STATE_WORDS; ++field) {
        root_states[field * root_count + root] = base_states[field * base_stride + game];
    }
    root_rng_states[root] = mix_stream_seed(
        seed, proposal_index, CANDIDATE_PROPOSAL_RNG_DOMAIN
    );
    root_chance_rng_states[root] = mix_stream_seed(
        seed, proposal_index, CANDIDATE_PROPOSAL_CHANCE_RNG_DOMAIN
    );
    generate_rollout_action_lane(
        root_states,
        topology,
        root_actions,
        root_rng_states,
        root_chance_rng_states,
        root_count,
        root
    );
}

static inline __device__ int root_actions_equal(
    const uint32_t *root_actions,
    uint32_t root_count,
    uint32_t left,
    uint32_t right
) {
    for (uint32_t field = 0u; field < ACTION_WORDS; ++field) {
        if (root_actions[field * root_count + left]
            != root_actions[field * root_count + right]) {
            return 0;
        }
    }
    return 1;
}

extern "C" __global__ void select_candidate_root_actions_kernel(
    const uint32_t *root_actions,
    const uint64_t *root_stats,
    const uint32_t *candidate_ready,
    uint32_t *actions,
    uint32_t *status,
    uint32_t game_stride,
    uint32_t game_count,
    uint32_t roots_per_game,
    uint32_t root_count
) {
    const uint32_t game = blockIdx.x * blockDim.x + threadIdx.x;
    if (game >= game_count || candidate_ready[game] == 0u || roots_per_game == 0u) {
        return;
    }
    const uint32_t start = game * roots_per_game;
    const uint32_t end = start + roots_per_game < root_count
        ? start + roots_per_game
        : root_count;
    uint32_t best_root = 0xffffffffu;
    long long best_net = 0ll;
    uint64_t best_samples = 1ull;
    long long best_margin = 0ll;
    long long best_strategic = 0ll;
    uint64_t best_valid = 1ull;
    uint64_t best_turns = 0ull;

    for (uint32_t root = start; root < end; ++root) {
        if (root_actions[ACTION_TAG * root_count + root] >= 254u) {
            continue;
        }
        int duplicate = 0;
        for (uint32_t previous = start; previous < root; ++previous) {
            if (root_actions[ACTION_TAG * root_count + previous] < 254u
                && root_actions_equal(root_actions, root_count, previous, root)) {
                duplicate = 1;
                break;
            }
        }
        if (duplicate) {
            continue;
        }

        uint64_t samples = 0ull;
        uint64_t errors = 0ull;
        uint64_t terminals = 0ull;
        uint64_t wins = 0ull;
        uint64_t turns = 0ull;
        uint64_t actor_vp = 0ull;
        uint64_t opponent_vp = 0ull;
        uint64_t strategic_sum = 0ull;
        for (uint32_t peer = root; peer < end; ++peer) {
            if (root_actions[ACTION_TAG * root_count + peer] >= 254u
                || !root_actions_equal(root_actions, root_count, root, peer)) {
                continue;
            }
            samples += root_stats[0u * root_count + peer];
            errors += root_stats[1u * root_count + peer];
            terminals += root_stats[2u * root_count + peer];
            wins += root_stats[3u * root_count + peer];
            turns += root_stats[4u * root_count + peer];
            actor_vp += root_stats[5u * root_count + peer];
            opponent_vp += root_stats[6u * root_count + peer];
            strategic_sum += root_stats[10u * root_count + peer];
        }
        if (samples == 0ull || errors >= samples) {
            continue;
        }
        const uint64_t valid = samples - errors;
        const long long net = (long long)(2ull * wins) - (long long)terminals;
        const long long margin = (long long)actor_vp - (long long)opponent_vp;
        const long long strategic = (long long)strategic_sum;

        int better = best_root == 0xffffffffu;
        if (!better) {
            const long long net_left = net * (long long)best_samples;
            const long long net_right = best_net * (long long)samples;
            if (net_left != net_right) {
                better = net_left > net_right;
            } else {
                const long long margin_left = strategic * (long long)best_valid;
                const long long margin_right = best_strategic * (long long)valid;
                if (margin_left != margin_right) {
                    better = margin_left > margin_right;
                } else {
                    const uint64_t turns_left = turns * best_valid;
                    const uint64_t turns_right = best_turns * valid;
                    if (turns_left != turns_right) {
                        better = turns_left < turns_right;
                    }
                }
            }
        }
        if (better) {
            best_root = root;
            best_net = net;
            best_samples = samples;
            best_margin = margin;
            best_strategic = strategic;
            best_valid = valid;
            best_turns = turns;
        }
    }

    if (best_root == 0xffffffffu) {
        status[game] = STATUS_INVALID_ACTION;
        return;
    }
    for (uint32_t field = 0u; field < ACTION_WORDS; ++field) {
        actions[field * game_stride + game]
            = root_actions[field * root_count + best_root];
    }
}

extern "C" __global__ void run_games_kernel(
    uint32_t *states,
    const uint32_t *topology,
    uint32_t *actions,
    uint32_t *status,
    uint64_t *rng_states,
    uint64_t *chance_rng_states,
    uint32_t *action_counts,
    uint32_t stride,
    uint32_t count,
    uint32_t max_actions,
    uint32_t max_turns
) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) {
        return;
    }
    uint32_t executed = 0u;
    for (; executed < max_actions && status[lane] == STATUS_OK; ++executed) {
        if (state_get(states, stride, STATE_PHASE, lane) == PHASE_FINISHED
            || state_get(states, stride, STATE_TURN, lane) >= max_turns) {
            break;
        }
        generate_rollout_action_lane(
            states, topology, actions, rng_states, chance_rng_states, stride, lane
        );
        apply_transition_lane(states, topology, actions, status, stride, lane);
    }
    action_counts[lane] = executed;
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
    const uint64_t *root_seed_keys,
    uint32_t root_count,
    uint32_t chunk_rollouts_per_action,
    uint32_t total_rollouts_per_action,
    uint32_t rollout_offset,
    uint32_t *states,
    uint32_t *actions,
    uint32_t *status,
    uint64_t *rng_states,
    uint64_t *chance_rng_states,
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
    for (uint32_t field = 0u; field < ACTION_WORDS; ++field) {
        actions[field * stride + lane] = root_actions[field * root_count + root];
    }
    status[lane] = STATUS_OK;
    const uint32_t local_rollout = lane % chunk_rollouts_per_action;
    const uint64_t rollout = (uint64_t)rollout_offset + (uint64_t)local_rollout;
    const uint64_t root_seed = root_seed_keys[root];
    rng_states[lane] = mix_stream_seed(seed ^ root_seed, rollout, ROOT_RNG_DOMAIN);
    chance_rng_states[lane] = mix_stream_seed(
        seed ^ root_seed, rollout, ROOT_CHANCE_RNG_DOMAIN
    );
    apply_transition_lane(states, topology, actions, status, stride, lane);
}

extern "C" __global__ void reduce_root_rollouts_kernel(
    const uint32_t *states,
    const uint32_t *status,
    const uint32_t *base_states,
    const uint32_t *topology,
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
    long long actor_strategic = 0;
    long long best_opponent_strategic = -0x7fffffffffffffffll;
    uint32_t winner = 0xffffffffu;
    for (uint32_t player = 0u; player < players; ++player) {
        const uint32_t vp = player_get(states, stride, lane, player, PLAYER_PUBLIC_VP)
            + player_get(states, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
        const long long strategic = rollout_cutoff_player_score(states, topology, stride, lane, player);
        if (player == actor) {
            actor_vp = vp;
            actor_strategic = strategic;
        } else {
            if (vp > best_opponent_vp) best_opponent_vp = vp;
            if (strategic > best_opponent_strategic) best_opponent_strategic = strategic;
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
    const long long margin = (long long)actor_vp - (long long)best_opponent_vp;
    atomicAdd(&stats[7u * root_count + root], (uint64_t)(margin * margin));
    atomicAdd(&stats[8u * root_count + root], (uint64_t)actor_vp * (uint64_t)actor_vp);
    atomicAdd(
        &stats[9u * root_count + root],
        (uint64_t)best_opponent_vp * (uint64_t)best_opponent_vp
    );
    const long long strategic_margin = actor_strategic - best_opponent_strategic;
    // Two's-complement modular addition preserves signed sums in the u64 ABI.
    atomicAdd(&stats[10u * root_count + root], (uint64_t)strategic_margin);
    atomicAdd(&stats[11u * root_count + root], (uint64_t)(strategic_margin * strategic_margin));
}

// Checked before any resident state or reduction buffer is used by Rust.
extern "C" __global__ void simulation_contract_kernel(uint32_t *out) {
    out[0] = 2u;
    out[1] = STATE_WORDS;
    out[2] = ACTION_WORDS;
    out[3] = 12u;
}
