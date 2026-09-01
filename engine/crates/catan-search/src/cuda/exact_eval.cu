// NVRTC provides CUDA device math intrinsics directly. Keeping this translation
// unit header-free avoids depending on a host CUDA toolkit include directory.
typedef unsigned int uint32_t;
typedef unsigned long long uint64_t;
#define EXACT_INFINITY 3.402823466e+38F

#define MAX_PLAYERS 4u
#define HEX_COUNT 19u
#define VERTEX_COUNT 54u
#define EDGE_COUNT 72u
#define MAX_VERTEX_ADJACENCY 3u

#define STATE_NUM_PLAYERS 0u
#define STATE_PHASE 1u
#define STATE_CURRENT_PLAYER 2u
#define STATE_ROBBER_HEX 3u
#define STATE_VICTORY_TARGET 4u
#define STATE_DISCARD_LIMIT 5u
#define STATE_BANK_PUBLIC 6u
#define STATE_LONGEST_HOLDER 7u
#define STATE_LARGEST_HOLDER 8u
#define STATE_BANK 9u
#define STATE_DEVELOPMENT_DECK 14u
#define STATE_PLAYED_DEVELOPMENT 19u
#define STATE_HEX_RESOURCES 24u
#define STATE_HEX_NUMBERS 43u
#define STATE_PORTS 62u
#define STATE_BUILDINGS 116u
#define STATE_ROADS 170u
#define STATE_PLAYERS 242u
#define PLAYER_STRIDE 22u
#define STATE_WORDS 330u

#define TOPO_VERTEX_HEX_COUNTS 0u
#define TOPO_VERTEX_HEXES 54u
#define TOPO_VERTEX_VERTEX_COUNTS 216u
#define TOPO_VERTEX_VERTICES 270u
#define TOPO_VERTEX_EDGE_COUNTS 432u
#define TOPO_VERTEX_EDGES 486u
#define TOPO_EDGE_VERTICES 648u

#define INVALID_DISTANCE 255u
#define INVALID_OWNER 0xffffffffu

static const float PIPS[13] = {
    0.0f, 0.0f, 1.0f, 2.0f, 3.0f, 4.0f, 5.0f,
    0.0f, 5.0f, 4.0f, 3.0f, 2.0f, 1.0f,
};

static const float BASE_RESOURCE_WEIGHTS[5] = {
    0.98f, 0.98f, 0.73f, 1.22f, 1.10f,
};

static const uint32_t BUILD_COSTS[4][5] = {
    {1u, 1u, 0u, 0u, 0u},
    {1u, 1u, 1u, 1u, 0u},
    {0u, 0u, 0u, 2u, 3u},
    {0u, 0u, 1u, 1u, 1u},
};

static const float PLAN_VALUE[4] = {
    0.35f, 1.25f, 1.15f, 0.72f,
};

static const float BUILD_TEMPO_VALUE[4] = {
    0.32f, 1.25f, 1.18f, 0.68f,
};

static const float COMPLETED_VALUE[4] = {
    0.35f, 1.45f, 1.35f, 0.78f,
};

static const float NEAR_PLAN_VALUE[4] = {
    0.25f, 1.10f, 1.0f, 0.58f,
};

static inline __device__ float clampf_exact(float value, float low, float high) {
    return fminf(high, fmaxf(low, value));
}

static inline __device__ float sigmoid_exact(float value) {
    return 1.0f / (1.0f + expf(-value));
}

static inline __device__ uint32_t player_base(uint32_t player) {
    return STATE_PLAYERS + player * PLAYER_STRIDE;
}

static inline __device__ uint32_t resource_count(
    const uint32_t *state,
    uint32_t player,
    uint32_t resource
) {
    return state[player_base(player) + resource];
}

static inline __device__ uint32_t development_count(
    const uint32_t *state,
    uint32_t player,
    uint32_t card
) {
    return state[player_base(player) + 5u + card];
}

static inline __device__ uint32_t bought_development_count(
    const uint32_t *state,
    uint32_t player,
    uint32_t card
) {
    return state[player_base(player) + 10u + card];
}

static inline __device__ uint32_t resource_total(
    const uint32_t *state,
    uint32_t player
) {
    uint32_t result = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        result += resource_count(state, player, resource);
    }
    return result;
}

static inline __device__ int building_player(uint32_t building) {
    if (building == 0u) {
        return -1;
    }
    if (building <= 4u) {
        return (int)building - 1;
    }
    if (building <= 8u) {
        return (int)building - 5;
    }
    return -1;
}

static inline __device__ uint32_t building_multiplier(uint32_t building) {
    return building >= 5u && building <= 8u ? 2u : 1u;
}

static inline __device__ uint32_t player_roads_left(
    const uint32_t *state,
    uint32_t player
) {
    return state[player_base(player) + 17u];
}

static inline __device__ uint32_t player_settlements_left(
    const uint32_t *state,
    uint32_t player
) {
    return state[player_base(player) + 18u];
}

static inline __device__ uint32_t player_cities_left(
    const uint32_t *state,
    uint32_t player
) {
    return state[player_base(player) + 19u];
}

static inline __device__ uint32_t player_public_victory_points(
    const uint32_t *state,
    uint32_t player
) {
    return state[player_base(player) + 15u];
}

static inline __device__ uint32_t player_victory_points(
    const uint32_t *state,
    uint32_t player
) {
    return player_public_victory_points(state, player) + development_count(state, player, 1u);
}

static inline __device__ uint32_t topo_vertex_hex_count(
    const uint32_t *topology,
    uint32_t vertex
) {
    return topology[TOPO_VERTEX_HEX_COUNTS + vertex];
}

static inline __device__ uint32_t topo_vertex_hex(
    const uint32_t *topology,
    uint32_t vertex,
    uint32_t slot
) {
    return topology[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot];
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

static inline __device__ uint32_t port_ratio_value(uint32_t port, uint32_t resource) {
    return port == resource + 2u ? 2u : 4u;
}

static inline __device__ void trade_ratios(
    const uint32_t *state,
    uint32_t player,
    uint32_t ratios[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        ratios[resource] = 4u;
    }
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        if ((uint32_t)building_player(state[STATE_BUILDINGS + vertex]) != player) {
            continue;
        }
        const uint32_t port = state[STATE_PORTS + vertex];
        if (port == 1u) {
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                ratios[resource] = ratios[resource] < 3u ? ratios[resource] : 3u;
            }
        } else if (port >= 2u && port <= 6u) {
            const uint32_t resource = port - 2u;
            ratios[resource] = 2u;
        }
    }
}

static inline __device__ float bank_supply_factor(
    const uint32_t *state,
    uint32_t resource
) {
    if (state[STATE_BANK_PUBLIC] == 0u) {
        return 1.0f;
    }
    const uint32_t available = state[STATE_BANK + resource];
    if (available == 0u) {
        return 0.15f;
    }
    if (available == 1u) {
        return 0.55f;
    }
    if (available == 2u) {
        return 0.82f;
    }
    return 1.0f;
}

static inline __device__ void production_pips(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    float production[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        production[resource] = 0.0f;
    }
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        const uint32_t building = state[STATE_BUILDINGS + vertex];
        if ((uint32_t)building_player(building) != player) {
            continue;
        }
        const uint32_t hex_count = topo_vertex_hex_count(topology, vertex);
        for (uint32_t slot = 0u; slot < hex_count; ++slot) {
            const uint32_t hex = topo_vertex_hex(topology, vertex, slot);
            const uint32_t encoded_resource = state[STATE_HEX_RESOURCES + hex];
            if (encoded_resource == 0u) {
                continue;
            }
            const uint32_t resource = encoded_resource - 1u;
            const float active = hex == state[STATE_ROBBER_HEX] ? 0.12f : 1.0f;
            const float supply = bank_supply_factor(state, resource);
            const uint32_t number = state[STATE_HEX_NUMBERS + hex];
            production[resource] +=
                PIPS[number] * (float)building_multiplier(building) * active * supply;
        }
    }
}

static inline __device__ void deficit(
    const uint32_t hand[5],
    uint32_t kind,
    uint32_t missing[5]
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t cost = BUILD_COSTS[kind][resource];
        missing[resource] = cost > hand[resource] ? cost - hand[resource] : 0u;
    }
}

static inline __device__ void dynamic_resource_weights(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    float weights[5]
) {
    uint32_t hand[5];
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        hand[resource] = resource_count(state, player, resource);
    }
    float production[5];
    production_pips(state, topology, player, production);
    uint32_t ratios[5];
    trade_ratios(state, player, ratios);

    float best_score = EXACT_INFINITY;
    uint32_t best_missing[5] = {0u, 0u, 0u, 0u, 0u};
    for (uint32_t kind = 0u; kind < 4u; ++kind) {
        uint32_t missing[5];
        deficit(hand, kind, missing);
        float weighted = 0.0f;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            const float scarcity = clampf_exact(
                6.0f / (production[resource] + 1.5f),
                0.55f,
                2.2f
            );
            weighted +=
                (float)missing[resource] * BASE_RESOURCE_WEIGHTS[resource] * scarcity;
        }
        const float score = weighted / fmaxf(PLAN_VALUE[kind], 0.1f);
        if (score < best_score) {
            best_score = score;
            for (uint32_t resource = 0u; resource < 5u; ++resource) {
                best_missing[resource] = missing[resource];
            }
        }
    }

    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const float scarcity = clampf_exact(
            6.0f / (production[resource] + 2.0f),
            0.65f,
            1.8f
        );
        const float bottleneck = best_missing[resource] > 0u
            ? 1.0f + 0.36f * (float)best_missing[resource]
            : 0.9f;
        const float port_liquidity = ratios[resource] == 2u
            ? 1.18f
            : (ratios[resource] == 3u ? 1.08f : 1.0f);
        const uint32_t surplus_cards = hand[resource] > 4u ? hand[resource] - 4u : 0u;
        weights[resource] = BASE_RESOURCE_WEIGHTS[resource] * scarcity
            * bottleneck * port_liquidity / (1.0f + (float)surplus_cards * 0.10f);
    }
}

static inline __device__ int contains_cost(
    const uint32_t hand[5],
    uint32_t kind
) {
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (hand[resource] < BUILD_COSTS[kind][resource]) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ uint32_t hand_total(const uint32_t hand[5]) {
    uint32_t total = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        total += hand[resource];
    }
    return total;
}

static inline __device__ float hand_utility_with_weights(
    const uint32_t hand[5],
    const float weights[5]
) {
    float liquidity = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        liquidity += (float)hand[resource] * weights[resource];
    }
    float completed = 0.0f;
    for (uint32_t kind = 0u; kind < 4u; ++kind) {
        if (contains_cost(hand, kind)) {
            completed = fmaxf(completed, COMPLETED_VALUE[kind]);
        }
    }
    float near_plan = 0.0f;
    for (uint32_t kind = 0u; kind < 4u; ++kind) {
        uint32_t missing[5];
        deficit(hand, kind, missing);
        const float missing_total = (float)hand_total(missing);
        near_plan = fmaxf(
            near_plan,
            NEAR_PLAN_VALUE[kind] / (1.0f + missing_total)
        );
    }
    return liquidity * 0.18f + completed + near_plan;
}

static inline __device__ uint32_t rolls_before_next_spend(
    const uint32_t *state,
    uint32_t player
) {
    const uint32_t current = state[STATE_CURRENT_PLAYER];
    const uint32_t phase = state[STATE_PHASE];
    if (current == player && phase == 7u) {
        return 0u;
    }
    const uint32_t count = state[STATE_NUM_PLAYERS];
    const uint32_t distance = (player + count - current) % count;
    const uint32_t pending = phase == 2u || phase == 3u ? 1u : 0u;
    if (distance == 0u) {
        return pending;
    }
    return distance + pending;
}

static inline __device__ float optimal_kept_utility(
    const uint32_t hand[5],
    uint32_t discard_count,
    const float weights[5]
) {
    float best = 0.0f;
    const uint32_t first_limit = discard_count < hand[0] ? discard_count : hand[0];
    for (uint32_t first = 0u; first <= first_limit; ++first) {
        const uint32_t after_first = discard_count - first;
        const uint32_t second_limit = after_first < hand[1] ? after_first : hand[1];
        for (uint32_t second = 0u; second <= second_limit; ++second) {
            const uint32_t after_second = after_first - second;
            const uint32_t third_limit = after_second < hand[2] ? after_second : hand[2];
            for (uint32_t third = 0u; third <= third_limit; ++third) {
                const uint32_t after_third = after_second - third;
                const uint32_t fourth_limit = after_third < hand[3] ? after_third : hand[3];
                for (uint32_t fourth = 0u; fourth <= fourth_limit; ++fourth) {
                    const uint32_t last = after_third - fourth;
                    if (last > hand[4]) {
                        continue;
                    }
                    const uint32_t discarded[5] = {
                        first, second, third, fourth, last,
                    };
                    uint32_t kept[5];
                    for (uint32_t resource = 0u; resource < 5u; ++resource) {
                        kept[resource] = hand[resource] - discarded[resource];
                    }
                    best = fmaxf(best, hand_utility_with_weights(kept, weights));
                }
            }
        }
    }
    return best;
}

static inline __device__ float expected_discard_loss(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player
) {
    uint32_t hand[5];
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        hand[resource] = resource_count(state, player, resource);
    }
    const uint32_t held = hand_total(hand);
    const uint32_t rolls = rolls_before_next_spend(state, player);
    if (rolls == 0u) {
        const uint32_t overflow = held > state[STATE_DISCARD_LIMIT]
            ? held - state[STATE_DISCARD_LIMIT]
            : 0u;
        return 0.04f * (float)overflow;
    }

    const float safe_probability = 5.0f / 6.0f;
    const float probability = 1.0f - powf(safe_probability, (float)rolls);
    float numerator = 0.0f;
    float survival = 1.0f;
    for (uint32_t safe_rolls = 0u; safe_rolls < rolls; ++safe_rolls) {
        numerator += (float)safe_rolls * survival * (1.0f / 6.0f);
        survival *= 5.0f / 6.0f;
    }
    const float expected_safe_rolls = numerator / fmaxf(probability, 1.1920928955078125e-7f);
    float production[5];
    production_pips(state, topology, player, production);
    uint32_t projected[5];
    float expected_added_total = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const float expected = production[resource] / 30.0f * expected_safe_rolls;
        expected_added_total += expected;
        projected[resource] = hand[resource] + (uint32_t)floorf(expected);
    }
    const uint32_t projected_held = hand_total(projected);
    if (projected_held <= state[STATE_DISCARD_LIMIT]) {
        const float excess = fmaxf(
            (float)held + expected_added_total - (float)state[STATE_DISCARD_LIMIT],
            0.0f
        );
        return probability * powf(excess, 1.35f) * 0.32f;
    }

    float weights[5];
    dynamic_resource_weights(state, topology, player, weights);
    const float before = hand_utility_with_weights(projected, weights);
    const float kept = optimal_kept_utility(projected, projected_held / 2u, weights);
    const float expected_cards_lost = probability * (float)(projected_held / 2u);
    const float overflow = (float)(projected_held - state[STATE_DISCARD_LIMIT]);
    return probability * fmaxf(before - kept, 0.0f)
        + expected_cards_lost * 0.22f
        + probability * powf(overflow, 1.35f) * 0.15f;
}

static inline __device__ void route_distances(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    uint32_t distances[VERTEX_COUNT]
) {
    uint32_t visited[VERTEX_COUNT];
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        distances[vertex] = INVALID_DISTANCE;
        visited[vertex] = 0u;
        if ((uint32_t)building_player(state[STATE_BUILDINGS + vertex]) == player) {
            distances[vertex] = 0u;
        }
    }
    for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
        if (state[STATE_ROADS + edge] != player + 1u) {
            continue;
        }
        const uint32_t first = topo_edge_vertex(topology, edge, 0u);
        const uint32_t second = topo_edge_vertex(topology, edge, 1u);
        distances[first] = 0u;
        distances[second] = 0u;
    }

    for (uint32_t iteration = 0u; iteration < VERTEX_COUNT; ++iteration) {
        uint32_t best_vertex = VERTEX_COUNT;
        uint32_t best_cost = INVALID_DISTANCE;
        for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
            if (visited[vertex] == 0u && distances[vertex] < best_cost) {
                best_cost = distances[vertex];
                best_vertex = vertex;
            }
        }
        if (best_vertex == VERTEX_COUNT) {
            break;
        }
        visited[best_vertex] = 1u;
        const int owner = building_player(state[STATE_BUILDINGS + best_vertex]);
        if (owner >= 0 && (uint32_t)owner != player) {
            continue;
        }
        const uint32_t edge_count = topo_vertex_edge_count(topology, best_vertex);
        for (uint32_t slot = 0u; slot < edge_count; ++slot) {
            const uint32_t edge = topo_vertex_edge(topology, best_vertex, slot);
            const uint32_t road = state[STATE_ROADS + edge];
            if (road != 0u && road != player + 1u) {
                continue;
            }
            const uint32_t first = topo_edge_vertex(topology, edge, 0u);
            const uint32_t second = topo_edge_vertex(topology, edge, 1u);
            const uint32_t next = first == best_vertex ? second : first;
            const uint32_t next_cost = best_cost + (road == 0u ? 1u : 0u);
            if (next_cost < distances[next]) {
                distances[next] = next_cost;
            }
        }
    }
}

static inline __device__ float turns_until_action(
    const uint32_t *state,
    uint32_t player
) {
    const uint32_t count = state[STATE_NUM_PLAYERS];
    const uint32_t current = state[STATE_CURRENT_PLAYER];
    const uint32_t seats = (player + count - current) % count;
    const uint32_t phase = state[STATE_PHASE];
    const int immediate_phase = phase == 7u || phase == 2u || phase == 9u;
    const float phase_delay = player == current && immediate_phase ? 0.0f : 0.15f;
    return (float)seats + phase_delay;
}

static inline __device__ float expansion_arrival_score(
    const uint32_t *state,
    uint32_t player,
    uint32_t roads_required,
    const float production[5],
    const uint32_t ratios[5]
) {
    uint32_t plan_cost[5] = {
        1u + roads_required,
        1u + roads_required,
        1u,
        1u,
        0u,
    };
    uint32_t missing[5];
    uint32_t missing_total = 0u;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        const uint32_t hand = resource_count(state, player, resource);
        missing[resource] = plan_cost[resource] > hand
            ? plan_cost[resource] - hand
            : 0u;
        missing_total += missing[resource];
    }
    if (missing_total == 0u) {
        return turns_until_action(state, player);
    }
    float production_total = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        production_total += production[resource];
    }
    float expected_rolls = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        expected_rolls += (float)missing[resource] * 36.0f
            / (production[resource] + production_total / (float)ratios[resource] + 0.65f);
    }
    const uint32_t count = state[STATE_NUM_PLAYERS] > 1u
        ? state[STATE_NUM_PLAYERS]
        : 1u;
    return turns_until_action(state, player)
        + expected_rolls / (float)count
        + (float)roads_required * 0.08f;
}

static inline __device__ void expansion_arrival_scores(
    const uint32_t *state,
    const uint32_t *topology,
    float arrivals[MAX_PLAYERS][16]
) {
    for (uint32_t player = 0u; player < MAX_PLAYERS; ++player) {
        for (uint32_t roads = 0u; roads < 16u; ++roads) {
            arrivals[player][roads] = 0.0f;
        }
    }
    const uint32_t count = state[STATE_NUM_PLAYERS];
    for (uint32_t player = 0u; player < count; ++player) {
        float production[5];
        production_pips(state, topology, player, production);
        uint32_t ratios[5];
        trade_ratios(state, player, ratios);
        const uint32_t roads_left = player_roads_left(state, player);
        for (uint32_t roads = 0u; roads <= roads_left && roads < 16u; ++roads) {
            arrivals[player][roads] = expansion_arrival_score(
                state,
                player,
                roads,
                production,
                ratios
            );
        }
    }
}

static inline __device__ int settlement_vertex_open(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t vertex
) {
    if (state[STATE_BUILDINGS + vertex] != 0u) {
        return 0;
    }
    const uint32_t count = topo_vertex_vertex_count(topology, vertex);
    for (uint32_t slot = 0u; slot < count; ++slot) {
        const uint32_t adjacent = topo_vertex_vertex(topology, vertex, slot);
        if (state[STATE_BUILDINGS + adjacent] != 0u) {
            return 0;
        }
    }
    return 1;
}

static inline __device__ float expansion_site_survival(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    uint32_t vertex,
    const uint32_t routes[MAX_PLAYERS][VERTEX_COUNT],
    const float arrivals[MAX_PLAYERS][16]
) {
    const uint32_t own_distance = routes[player][vertex];
    const float own_arrival = arrivals[player][own_distance];
    float opponent_arrival = EXACT_INFINITY;
    const uint32_t count = state[STATE_NUM_PLAYERS];
    for (uint32_t candidate = 0u; candidate < count; ++candidate) {
        if (candidate == player) {
            continue;
        }
        const uint32_t distance = routes[candidate][vertex];
        if (distance != INVALID_DISTANCE
            && distance <= player_roads_left(state, candidate)
            && player_settlements_left(state, candidate) > 0u) {
            opponent_arrival = fminf(opponent_arrival, arrivals[candidate][distance]);
        }
    }
    if (opponent_arrival == EXACT_INFINITY) {
        return 0.985f;
    }
    if (own_arrival <= 0.01f) {
        return 0.995f;
    }
    return clampf_exact(
        sigmoid_exact((opponent_arrival - own_arrival) * 1.35f - 0.10f),
        0.01f,
        0.995f
    );
}

static inline __device__ float vertex_value_with_weights(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t vertex,
    const float weights[5],
    uint32_t player
) {
    float value = 0.0f;
    uint32_t numbers = 0u;
    uint32_t resources = 0u;
    const uint32_t hex_count = topo_vertex_hex_count(topology, vertex);
    for (uint32_t slot = 0u; slot < hex_count; ++slot) {
        const uint32_t hex = topo_vertex_hex(topology, vertex, slot);
        const uint32_t encoded_resource = state[STATE_HEX_RESOURCES + hex];
        if (encoded_resource == 0u) {
            continue;
        }
        const uint32_t resource = encoded_resource - 1u;
        const uint32_t number = state[STATE_HEX_NUMBERS + hex];
        const float robber_factor = hex == state[STATE_ROBBER_HEX] ? 0.30f : 1.0f;
        value += PIPS[number] * weights[resource] * robber_factor;
        numbers |= 1u << number;
        resources |= 1u << resource;
    }
    value += (float)__popc(numbers) * 0.16f;
    value += (float)__popc(resources) * 0.22f;
    if (state[STATE_PORTS + vertex] != 0u) {
        uint32_t ratios[5];
        trade_ratios(state, player, ratios);
        int all_four = 1;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            if (ratios[resource] != 4u) {
                all_four = 0;
                break;
            }
        }
        value += all_four ? 0.7f : 0.0f;
    }
    return value;
}

static inline __device__ void expansion_option_value(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    const uint32_t routes[MAX_PLAYERS][VERTEX_COUNT],
    const float arrivals[MAX_PLAYERS][16],
    const float weights[5],
    float *value,
    float *portfolio
) {
    *value = 0.0f;
    *portfolio = 0.0f;
    if (player_settlements_left(state, player) == 0u) {
        return;
    }
    const uint32_t own_roads_left = player_roads_left(state, player);
    const uint32_t *distances = routes[player];
    float top[3] = {0.0f, 0.0f, 0.0f};
    const uint32_t arrival_count = state[STATE_NUM_PLAYERS];
    (void)arrival_count;
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        const uint32_t distance = distances[vertex];
        if (!settlement_vertex_open(state, topology, vertex)
            || distance == INVALID_DISTANCE
            || distance > own_roads_left) {
            continue;
        }
        const float survival = expansion_site_survival(
            state,
            topology,
            player,
            vertex,
            routes,
            arrivals
        );
        const float site = vertex_value_with_weights(
            state,
            topology,
            vertex,
            weights,
            player
        );
        const float road_cost = (float)distance * 1.45f;
        const float candidate = survival * (site + 5.4f) / (1.0f + road_cost * 0.34f);
        if (candidate > top[0]) {
            top[2] = top[1];
            top[1] = top[0];
            top[0] = candidate;
        } else if (candidate > top[1]) {
            top[2] = top[1];
            top[1] = candidate;
        } else if (candidate > top[2]) {
            top[2] = candidate;
        }
        if (candidate > *value) {
            *value = candidate;
        }
    }
    *portfolio = top[1] * 0.55f + top[2] * 0.30f;
}

static inline __device__ int road_bit_used(
    uint64_t low,
    uint64_t high,
    uint32_t edge
) {
    if (edge < 64u) {
        return (low & (1ull << edge)) != 0ull;
    }
    return (high & (1ull << (edge - 64u))) != 0ull;
}

static inline __device__ void road_bit_set(
    uint64_t *low,
    uint64_t *high,
    uint32_t edge
) {
    if (edge < 64u) {
        *low |= 1ull << edge;
    } else {
        *high |= 1ull << (edge - 64u);
    }
}

struct RoadFrame {
    uint32_t vertex;
    uint32_t incoming_edge;
    uint32_t next_slot;
    uint32_t best_tail;
    uint64_t used_low;
    uint64_t used_high;
};

static inline __device__ uint32_t road_walk_length(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    uint32_t edge,
    uint32_t through
) {
    if (edge >= EDGE_COUNT || state[STATE_ROADS + edge] != player + 1u) {
        return 0u;
    }
    uint64_t used_low = 0ull;
    uint64_t used_high = 0ull;
    road_bit_set(&used_low, &used_high, edge);
    const uint32_t first = topo_edge_vertex(topology, edge, 0u);
    const uint32_t second = topo_edge_vertex(topology, edge, 1u);
    const uint32_t next_vertex = first == through ? second : first;
    const int next_owner = building_player(state[STATE_BUILDINGS + next_vertex]);
    if (next_owner >= 0 && (uint32_t)next_owner != player) {
        return 1u;
    }

    RoadFrame stack[EDGE_COUNT + 1u];
    uint32_t stack_size = 1u;
    stack[0].vertex = next_vertex;
    stack[0].incoming_edge = edge;
    stack[0].next_slot = 0u;
    stack[0].best_tail = 0u;
    stack[0].used_low = used_low;
    stack[0].used_high = used_high;

    while (stack_size > 0u) {
        RoadFrame *frame = &stack[stack_size - 1u];
        const uint32_t edge_count = topo_vertex_edge_count(topology, frame->vertex);
        if (frame->next_slot >= edge_count) {
            const uint32_t result = 1u + frame->best_tail;
            --stack_size;
            if (stack_size == 0u) {
                return result;
            }
            if (result > stack[stack_size - 1u].best_tail) {
                stack[stack_size - 1u].best_tail = result;
            }
            continue;
        }

        const uint32_t next_edge = topo_vertex_edge(
            topology,
            frame->vertex,
            frame->next_slot
        );
        ++frame->next_slot;
        if (next_edge == frame->incoming_edge
            || state[STATE_ROADS + next_edge] != player + 1u
            || road_bit_used(frame->used_low, frame->used_high, next_edge)) {
            continue;
        }

        uint64_t child_low = frame->used_low;
        uint64_t child_high = frame->used_high;
        road_bit_set(&child_low, &child_high, next_edge);
        const uint32_t next_first = topo_edge_vertex(topology, next_edge, 0u);
        const uint32_t next_second = topo_edge_vertex(topology, next_edge, 1u);
        const uint32_t child_vertex = next_first == frame->vertex
            ? next_second
            : next_first;
        const int child_owner = building_player(state[STATE_BUILDINGS + child_vertex]);
        if (child_owner >= 0 && (uint32_t)child_owner != player) {
            if (1u > frame->best_tail) {
                frame->best_tail = 1u;
            }
            continue;
        }
        if (stack_size >= EDGE_COUNT + 1u) {
            continue;
        }
        stack[stack_size].vertex = child_vertex;
        stack[stack_size].incoming_edge = next_edge;
        stack[stack_size].next_slot = 0u;
        stack[stack_size].best_tail = 0u;
        stack[stack_size].used_low = child_low;
        stack[stack_size].used_high = child_high;
        ++stack_size;
    }
    return 0u;
}

static inline __device__ uint32_t longest_road_length(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player
) {
    uint32_t best = 0u;
    for (uint32_t edge = 0u; edge < EDGE_COUNT; ++edge) {
        if (state[STATE_ROADS + edge] != player + 1u) {
            continue;
        }
        const uint32_t first = topo_edge_vertex(topology, edge, 0u);
        const uint32_t second = topo_edge_vertex(topology, edge, 1u);
        const uint32_t from_first = road_walk_length(
            state,
            topology,
            player,
            edge,
            first
        );
        const uint32_t from_second = road_walk_length(
            state,
            topology,
            player,
            edge,
            second
        );
        best = best > from_first ? best : from_first;
        best = best > from_second ? best : from_second;
    }
    return best;
}

static inline __device__ void longest_road_outlook(
    const uint32_t *state,
    uint32_t player,
    const uint32_t road_lengths[MAX_PLAYERS],
    float *acquire,
    float *retain,
    float *additional_cost
) {
    const float own = (float)road_lengths[player];
    float best_other = 0.0f;
    const uint32_t count = state[STATE_NUM_PLAYERS];
    for (uint32_t candidate = 0u; candidate < count; ++candidate) {
        if (candidate != player) {
            best_other = fmaxf(best_other, (float)road_lengths[candidate]);
        }
    }
    const float threshold = fmaxf(5.0f, best_other + 1.0f);
    const float missing = fmaxf(threshold - own, 0.0f);
    *acquire = state[STATE_LONGEST_HOLDER] == player + 1u
        ? 1.0f
        : sigmoid_exact((own - threshold + 0.5f) * 1.25f);
    *retain = sigmoid_exact((own - best_other - 0.5f) * 1.1f);
    *additional_cost = missing * 2.0f;
}

static inline __device__ void largest_army_outlook(
    const uint32_t *state,
    uint32_t player,
    float *acquire,
    float *retain,
    float *additional_cost
) {
    const float own = (float)state[player_base(player) + 16u];
    const uint32_t development_knights = development_count(state, player, 0u);
    const uint32_t bought_knights = bought_development_count(state, player, 0u);
    const float ready = (float)(development_knights > bought_knights
        ? development_knights - bought_knights
        : 0u);
    const float potential = own + ready;
    float best_other = 0.0f;
    const uint32_t count = state[STATE_NUM_PLAYERS];
    for (uint32_t candidate = 0u; candidate < count; ++candidate) {
        if (candidate != player) {
            best_other = fmaxf(
                best_other,
                (float)state[player_base(candidate) + 16u]
            );
        }
    }
    const float threshold = fmaxf(3.0f, best_other + 1.0f);
    const float missing = fmaxf(threshold - potential, 0.0f);
    const float remaining_knights = (float)state[STATE_DEVELOPMENT_DECK];
    const float deck_support = missing <= 0.0f
        ? 1.0f
        : clampf_exact(
            remaining_knights / (missing * (float)count + 1.0f),
            0.0f,
            1.0f
        );
    *acquire = state[STATE_LARGEST_HOLDER] == player + 1u
        ? 1.0f
        : sigmoid_exact((potential - threshold + 0.4f) * 1.35f) * deck_support;
    *retain = sigmoid_exact((potential - best_other - 0.4f) * 1.1f);
    *additional_cost = missing * 3.0f;
}

static inline __device__ float progress_card_utility(
    const uint32_t *state,
    uint32_t player,
    uint32_t card,
    float expansion_value
) {
    const float held = (float)development_count(state, player, card);
    if (held <= 0.0f) {
        return 0.0f;
    }
    const float congestion = 1.0f / (1.0f + fmaxf(held - 1.0f, 0.0f) * 0.55f);
    float base = 0.0f;
    if (card == 2u) {
        base = 0.55f + fminf(expansion_value, 4.0f) * 0.16f;
    } else if (card == 3u) {
        uint32_t hand[5];
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            hand[resource] = resource_count(state, player, resource);
        }
        uint32_t nearest = 0xffffffffu;
        for (uint32_t kind = 0u; kind < 4u; ++kind) {
            uint32_t missing[5];
            deficit(hand, kind, missing);
            nearest = nearest < hand_total(missing) ? nearest : hand_total(missing);
        }
        const uint32_t capped_nearest = nearest < 2u ? nearest : 2u;
        base = 0.65f + (2.0f - (float)capped_nearest) * 0.28f;
    } else if (card == 4u) {
        float opponents = 0.0f;
        const uint32_t count = state[STATE_NUM_PLAYERS];
        for (uint32_t candidate = 0u; candidate < count; ++candidate) {
            if (candidate != player) {
                opponents += (float)resource_total(state, candidate);
            }
        }
        base = 0.48f + opponents * 0.035f;
    }
    return fminf(held, 1.0f) * base
        + fmaxf(held - 1.0f, 0.0f) * base * 0.28f * congestion;
}

static inline __device__ float development_utility(
    const uint32_t *state,
    uint32_t player,
    float expansion_value
) {
    float army_acquire;
    float army_retain;
    float army_cost;
    largest_army_outlook(
        state,
        player,
        &army_acquire,
        &army_retain,
        &army_cost
    );
    (void)army_retain;
    (void)army_cost;
    const float knights = (float)development_count(state, player, 0u);
    const float knight_utility = fminf(knights, 1.0f)
        * (0.28f + army_acquire * 1.15f)
        + fmaxf(knights - 1.0f, 0.0f) * (0.12f + army_acquire * 0.24f);
    const float raw = knight_utility
        + progress_card_utility(state, player, 2u, expansion_value)
        + progress_card_utility(state, player, 3u, expansion_value)
        + progress_card_utility(state, player, 4u, expansion_value);
    const uint32_t action_cards = development_count(state, player, 0u)
        + development_count(state, player, 2u)
        + development_count(state, player, 3u)
        + development_count(state, player, 4u);
    const uint32_t newly_bought = bought_development_count(state, player, 0u)
        + bought_development_count(state, player, 2u)
        + bought_development_count(state, player, 3u)
        + bought_development_count(state, player, 4u);
    const uint32_t victory_points = player_victory_points(state, player);
    const uint32_t target = state[STATE_VICTORY_TARGET];
    const uint32_t usable_horizon = (target > victory_points ? target - victory_points : 0u) + 1u;
    const float queue = (float)(action_cards > 1u ? action_cards - 1u : 0u);
    const float horizon_excess = (float)(action_cards > usable_horizon
        ? action_cards - usable_horizon
        : 0u);
    return raw / (1.0f + queue * 0.20f + (float)newly_bought * 0.16f
        + horizon_excess * 0.30f);
}

static inline __device__ float expected_build_tempo(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player
) {
    uint32_t hand[5];
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        hand[resource] = resource_count(state, player, resource);
    }
    float production[5];
    production_pips(state, topology, player, production);
    uint32_t ratios[5];
    trade_ratios(state, player, ratios);
    float production_total = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        production_total += production[resource];
    }
    float best = 0.0f;
    for (uint32_t kind = 0u; kind < 4u; ++kind) {
        uint32_t missing[5];
        deficit(hand, kind, missing);
        float eta = 0.0f;
        for (uint32_t resource = 0u; resource < 5u; ++resource) {
            if (missing[resource] == 0u) {
                continue;
            }
            eta += (float)missing[resource] * 36.0f
                / (production[resource]
                    + production_total / (float)ratios[resource]
                    + 0.75f);
        }
        best = fmaxf(best, BUILD_TEMPO_VALUE[kind] / (1.0f + eta / 18.0f));
    }
    return best;
}

static inline __device__ float speculative_road_penalty(
    const uint32_t *state,
    uint32_t player,
    float road_acquire,
    float road_retain
) {
    const uint32_t roads_left = player_roads_left(state, player);
    const uint32_t roads_built = roads_left < 15u ? 15u - roads_left : 0u;
    uint32_t buildings = 0u;
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        if ((uint32_t)building_player(state[STATE_BUILDINGS + vertex]) == player) {
            ++buildings;
        }
    }
    uint32_t award_allowance = 0u;
    if (state[STATE_LONGEST_HOLDER] == player + 1u) {
        award_allowance = 3u;
    } else if (road_acquire * road_retain >= 0.55f) {
        award_allowance = 2u;
    }
    const uint32_t supported = buildings + 2u + award_allowance;
    const uint32_t excess = roads_built > supported ? roads_built - supported : 0u;
    const float excess_f = (float)excess;
    return excess_f * 0.48f + excess_f * excess_f * 0.035f;
}

static inline __device__ float strategic_utility(
    const uint32_t *state,
    const uint32_t *topology,
    uint32_t player,
    const uint32_t routes[MAX_PLAYERS][VERTEX_COUNT],
    const float arrivals[MAX_PLAYERS][16],
    const uint32_t road_lengths[MAX_PLAYERS]
) {
    const float victory = (float)player_victory_points(state, player);
    float production[5];
    production_pips(state, topology, player, production);
    float weights[5];
    dynamic_resource_weights(state, topology, player, weights);
    float weighted_production = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        weighted_production += production[resource] * weights[resource];
    }

    uint32_t distinct_numbers = 0u;
    float resource_diversity = 0.0f;
    for (uint32_t vertex = 0u; vertex < VERTEX_COUNT; ++vertex) {
        if ((uint32_t)building_player(state[STATE_BUILDINGS + vertex]) != player) {
            continue;
        }
        const uint32_t hex_count = topo_vertex_hex_count(topology, vertex);
        for (uint32_t slot = 0u; slot < hex_count; ++slot) {
            const uint32_t hex = topo_vertex_hex(topology, vertex, slot);
            const uint32_t number = state[STATE_HEX_NUMBERS + hex];
            if (number != 0u) {
                distinct_numbers |= 1u << number;
            }
        }
    }
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        if (production[resource] > 0.0f) {
            resource_diversity += 1.0f;
        }
    }
    uint32_t hand[5];
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        hand[resource] = resource_count(state, player, resource);
    }
    const float hand_value = hand_utility_with_weights(hand, weights);

    float expansion_value;
    float expansion_portfolio;
    expansion_option_value(
        state,
        topology,
        player,
        routes,
        arrivals,
        weights,
        &expansion_value,
        &expansion_portfolio
    );

    float road_acquire;
    float road_retain;
    float road_cost;
    longest_road_outlook(
        state,
        player,
        road_lengths,
        &road_acquire,
        &road_retain,
        &road_cost
    );
    (void)road_cost;
    float army_acquire;
    float army_retain;
    float army_cost;
    largest_army_outlook(
        state,
        player,
        &army_acquire,
        &army_retain,
        &army_cost
    );
    (void)army_cost;

    uint32_t ratios[5];
    trade_ratios(state, player, ratios);
    float port_flexibility = 0.0f;
    for (uint32_t resource = 0u; resource < 5u; ++resource) {
        port_flexibility += (float)(4u - ratios[resource]);
    }
    const uint32_t target = state[STATE_VICTORY_TARGET];
    const uint32_t victory_points = player_victory_points(state, player);
    const float points_to_win = (float)(target > victory_points
        ? target - victory_points
        : 0u);
    const float race_urgency = 1.0f + fmaxf(4.0f - points_to_win, 0.0f) * 0.18f;

    return victory * 7.4f
        + weighted_production * 0.17f
        + (float)__popc(distinct_numbers) * 0.06f
        + resource_diversity * 0.09f
        + hand_value * 0.48f
        + expected_build_tempo(state, topology, player) * 1.15f
        + expansion_value * 0.32f
        + expansion_portfolio * 0.22f
        + (road_acquire * road_retain) * 3.2f * race_urgency
        + (army_acquire * army_retain) * 3.2f * race_urgency
        + development_utility(state, player, expansion_value) * 0.72f
        + port_flexibility * 0.07f
        - expected_discard_loss(state, topology, player) * 2.4f
        - speculative_road_penalty(state, player, road_acquire, road_retain);
}

extern "C" __global__ void evaluate_batch_kernel(
    const uint32_t *states,
    const uint32_t *topology,
    float *outputs,
    uint32_t batch
) {
    const uint32_t index = blockIdx.x * blockDim.x + threadIdx.x;
    if (index >= batch) {
        return;
    }
    const uint32_t *state = states + index * STATE_WORDS;
    float result[MAX_PLAYERS] = {0.0f, 0.0f, 0.0f, 0.0f};
    const uint32_t count = state[STATE_NUM_PLAYERS];

    int winner = -1;
    if (state[STATE_PHASE] == 10u) {
        for (uint32_t player = 0u; player < count; ++player) {
            if (player_victory_points(state, player) >= state[STATE_VICTORY_TARGET]) {
                winner = (int)player;
                break;
            }
        }
    }
    if (winner >= 0) {
        result[winner] = 1.0f;
    } else {
        uint32_t routes[MAX_PLAYERS][VERTEX_COUNT];
        float arrivals[MAX_PLAYERS][16];
        uint32_t road_lengths[MAX_PLAYERS] = {0u, 0u, 0u, 0u};
        for (uint32_t player = 0u; player < count; ++player) {
            route_distances(state, topology, player, routes[player]);
            road_lengths[player] = longest_road_length(state, topology, player);
        }
        expansion_arrival_scores(state, topology, arrivals);
        float logits[MAX_PLAYERS];
        float maximum = -EXACT_INFINITY;
        for (uint32_t player = 0u; player < count; ++player) {
            logits[player] = strategic_utility(
                state,
                topology,
                player,
                routes,
                arrivals,
                road_lengths
            );
            maximum = fmaxf(maximum, logits[player]);
        }
        float total = 0.0f;
        for (uint32_t player = 0u; player < count; ++player) {
            result[player] = expf((logits[player] - maximum) * 0.50f);
            total += result[player];
        }
        if (total > 0.0f) {
            for (uint32_t player = 0u; player < count; ++player) {
                result[player] /= total;
            }
        }
    }
    for (uint32_t player = 0u; player < MAX_PLAYERS; ++player) {
        outputs[index * MAX_PLAYERS + player] = result[player];
    }
}
