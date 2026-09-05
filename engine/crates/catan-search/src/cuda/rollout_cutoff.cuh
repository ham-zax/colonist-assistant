// Integer endpoint evaluator. Mirrored by rollout_cutoff.rs; CPU/CUDA parity
// tests compare exact fixed-point results. Included after pips_for_number().
static inline __device__ void cutoff_production(
    const uint32_t *s, const uint32_t *topology, uint32_t stride,
    uint32_t lane, uint32_t player, uint32_t out[5]
) {
    for (uint32_t r = 0; r < 5; ++r) out[r] = 0;
    const uint32_t robber = state_get(s, stride, STATE_ROBBER_HEX, lane);
    for (uint32_t v = 0; v < VERTEX_COUNT; ++v) {
        const uint32_t b = state_get(s, stride, STATE_BUILDINGS + v, lane);
        if (building_player(b) != player) continue;
        const uint32_t n = topology[TOPO_VERTEX_HEX_COUNTS + v];
        for (uint32_t j = 0; j < n; ++j) {
            const uint32_t h = topology[TOPO_VERTEX_HEXES + v * MAX_VERTEX_ADJACENCY + j];
            const uint32_t encoded = state_get(s, stride, STATE_HEX_RESOURCES + h, lane);
            if (encoded == 0) continue;
            out[encoded - 1] += pips_for_number(state_get(s, stride, STATE_HEX_NUMBERS + h, lane))
                * building_multiplier(b) * (h == robber ? 1u : 8u);
        }
    }
}

static inline __device__ int cutoff_distance_open(
    const uint32_t *s, const uint32_t *topology, uint32_t stride,
    uint32_t lane, uint32_t v
) {
    if (state_get(s, stride, STATE_BUILDINGS + v, lane) != 0) return 0;
    for (uint32_t j = 0; j < topo_vertex_vertex_count(topology, v); ++j) {
        if (state_get(s, stride, STATE_BUILDINGS + topo_vertex_vertex(topology, v, j), lane) != 0) return 0;
    }
    return 1;
}

static inline __device__ int cutoff_touches_network(
    const uint32_t *s, const uint32_t *topology, uint32_t stride,
    uint32_t lane, uint32_t player, uint32_t v, uint32_t extra_edge
) {
    if (building_player(state_get(s, stride, STATE_BUILDINGS + v, lane)) == player) return 1;
    for (uint32_t j = 0; j < topo_vertex_edge_count(topology, v); ++j) {
        const uint32_t e = topo_vertex_edge(topology, v, j);
        if (e == extra_edge || state_get(s, stride, STATE_ROADS + e, lane) == player + 1) return 1;
    }
    return 0;
}

static inline __device__ int cutoff_route_distance(
    const uint32_t *s, const uint32_t *topology, uint32_t stride,
    uint32_t lane, uint32_t player, uint32_t v, uint32_t extra_edge
) {
    for (uint32_t j = 0; j < topo_vertex_edge_count(topology, v); ++j) {
        const uint32_t e = topo_vertex_edge(topology, v, j);
        if (e == extra_edge || state_get(s, stride, STATE_ROADS + e, lane) == player + 1) return 0;
    }
    for (uint32_t j = 0; j < topo_vertex_edge_count(topology, v); ++j) {
        const uint32_t e = topo_vertex_edge(topology, v, j);
        if (e == extra_edge || state_get(s, stride, STATE_ROADS + e, lane) != 0) continue;
        const uint32_t a = topo_edge_vertex(topology, e, 0);
        const uint32_t b = topo_edge_vertex(topology, e, 1);
        const uint32_t other = a == v ? b : a;
        const uint32_t owner = building_player(state_get(s, stride, STATE_BUILDINGS + other, lane));
        if (owner != 0xffffffffu && owner != player) continue;
        if (cutoff_touches_network(s, topology, stride, lane, player, other, extra_edge)) return 1;
    }
    return -1;
}

static inline __device__ uint32_t cutoff_expansion(
    const uint32_t *s, const uint32_t *topology, uint32_t stride,
    uint32_t lane, uint32_t player, uint32_t extra_edge
) {
    if (player_get(s, stride, lane, player, PLAYER_SETTLEMENTS_LEFT) == 0) return 0;
    uint32_t production[5];
    cutoff_production(s, topology, stride, lane, player, production);
    uint32_t missing_mask = 0;
    for (uint32_t r = 0; r < 4; ++r) if (production[r] == 0) missing_mask |= 1u << r;
    const int closed = state_get(s, stride, STATE_PLAYER_TRADES_ENABLED, lane) == 0
        || (state_get(s, stride, STATE_DOMESTIC_TRADE_DISABLED, lane) & (1u << player)) != 0;
    const uint32_t closure_weight = closed ? 64u : 40u;
    const uint32_t robber = state_get(s, stride, STATE_ROBBER_HEX, lane);
    uint32_t best = 0;
    for (uint32_t v = 0; v < VERTEX_COUNT; ++v) {
        if (!cutoff_distance_open(s, topology, stride, lane, v)) continue;
        const int distance = cutoff_route_distance(s, topology, stride, lane, player, v, extra_edge);
        if (distance < 0 || (uint32_t)distance > player_get(s, stride, lane, player, PLAYER_ROADS_LEFT)) continue;
        uint32_t pips = 0, mask = 0;
        for (uint32_t j = 0; j < topology[TOPO_VERTEX_HEX_COUNTS + v]; ++j) {
            const uint32_t h = topology[TOPO_VERTEX_HEXES + v * MAX_VERTEX_ADJACENCY + j];
            const uint32_t encoded = state_get(s, stride, STATE_HEX_RESOURCES + h, lane);
            if (encoded == 0) continue;
            pips += pips_for_number(state_get(s, stride, STATE_HEX_NUMBERS + h, lane)) * (h == robber ? 1u : 8u);
            mask |= 1u << (encoded - 1);
        }
        const uint32_t port = state_get(s, stride, STATE_PORTS + v, lane) != 0 ? 16u : 0u;
        const uint32_t score = (pips * 2u + __popc(mask) * 18u
            + __popc(mask & missing_mask) * closure_weight + port) / ((uint32_t)distance + 1u);
        best = score > best ? score : best;
    }
    return best < 320u ? best : 320u;
}

static inline __device__ uint32_t cutoff_build_access(
    uint32_t weight, int enabled, const uint32_t hand[5],
    const uint32_t ratios[5], const uint32_t bank[5], const uint32_t cost[5]
) {
    if (!enabled) return 0;
    uint32_t missing = 0, capacity = 0, unavailable = 0;
    for (uint32_t r = 0; r < 5; ++r) {
        const uint32_t reserved = hand[r] < cost[r] ? hand[r] : cost[r];
        const uint32_t deficit = cost[r] - reserved;
        missing += deficit;
        unavailable += deficit > bank[r] ? deficit - bank[r] : 0;
        capacity += (hand[r] - reserved) / (ratios[r] > 0 ? ratios[r] : 1u);
    }
    const uint32_t residual = missing > capacity ? missing - capacity : 0;
    return weight / (1u + (residual > unavailable ? residual : unavailable));
}

static inline __device__ long long rollout_cutoff_player_score(
    const uint32_t *s, const uint32_t *topology, uint32_t stride,
    uint32_t lane, uint32_t player
) {
    uint32_t production[5], hand[5], ratios[5], bank[5];
    cutoff_production(s, topology, stride, lane, player, production);
    uint32_t total = 0, diversity = 0, best_ratio = 0xffffffffu;
    for (uint32_t r = 0; r < 5; ++r) {
        total += production[r];
        diversity += production[r] > 0 ? 1 : 0;
        hand[r] = player_get(s, stride, lane, player, PLAYER_RESOURCES + r);
        ratios[r] = trade_ratio(s, stride, lane, player, r);
        bank[r] = state_get(s, stride, STATE_BANK + r, lane);
        if (production[r] > 0 && ratios[r] < best_ratio) best_ratio = ratios[r];
    }
    const uint32_t hand_total = hand_total_fixed(hand);
    const uint32_t limit = state_get(s, stride, STATE_DISCARD_LIMIT, lane);
    const uint32_t excess = hand_total > limit ? hand_total - limit : 0;
    uint32_t inventory = 0, deck = 0;
    for (uint32_t c = 0; c < 5; ++c) {
        deck += state_get(s, stride, STATE_DEVELOPMENT_DECK + c, lane);
        if (c != 1) inventory += player_get(s, stride, lane, player, PLAYER_DEVELOPMENT + c);
    }
    int has_settlement = 0, settlement_target = 0;
    for (uint32_t v = 0; v < VERTEX_COUNT; ++v) {
        has_settlement |= state_get(s, stride, STATE_BUILDINGS + v, lane) == player + 1;
        settlement_target |= cutoff_distance_open(s, topology, stride, lane, v)
            && cutoff_route_distance(s, topology, stride, lane, player, v, 0xffffffffu) == 0;
    }
    uint32_t independence = 0;
    if (best_ratio != 0xffffffffu) {
        uint32_t effective = 0;
        for (uint32_t r = 0; r < 5; ++r) effective += SETTLEMENT_COST[r] * (production[r] > 0 ? 1u : best_ratio);
        independence = 1280u / (effective > 4u ? effective : 4u);
    }
    const uint32_t vp = player_get(s, stride, lane, player, PLAYER_PUBLIC_VP)
        + player_get(s, stride, lane, player, PLAYER_DEVELOPMENT + 1u);
    long long score = (long long)vp * 1024ll + (long long)total * 4ll
        + (long long)diversity * 20ll + (long long)(hand_total < 12 ? hand_total : 12) * 10ll
        - (long long)excess * 28ll + (long long)inventory * 28ll
        + (long long)longest_road_length(s, topology, stride, lane, player) * 16ll
        + (long long)player_get(s, stride, lane, player, PLAYER_PLAYED_KNIGHTS) * 16ll;
    score += cutoff_build_access(90, player_get(s, stride, lane, player, PLAYER_ROADS_LEFT) > 0, hand, ratios, bank, ROAD_COST);
    score += cutoff_build_access(360, player_get(s, stride, lane, player, PLAYER_SETTLEMENTS_LEFT) > 0 && settlement_target, hand, ratios, bank, SETTLEMENT_COST);
    score += cutoff_build_access(320, player_get(s, stride, lane, player, PLAYER_CITIES_LEFT) > 0 && has_settlement, hand, ratios, bank, CITY_COST);
    score += cutoff_build_access(180, deck > 0, hand, ratios, bank, DEVELOPMENT_COST);
    return score + independence + cutoff_expansion(s, topology, stride, lane, player, 0xffffffffu);
}

// Availability lower bound is a policy input, not the sampled bank state.
static inline __device__ uint32_t observed_bank_lower_bound(
    const uint32_t *s, uint32_t stride, uint32_t lane,
    uint32_t player, uint32_t resource
) {
    if (state_get(s, stride, STATE_BANK_PUBLIC, lane) != 0) return state_get(s, stride, STATE_BANK + resource, lane);
    uint32_t outside = player_get(s, stride, lane, player, PLAYER_RESOURCES + resource);
    for (uint32_t p = 0; p < state_get(s, stride, STATE_NUM_PLAYERS, lane); ++p) {
        if (p != player) outside += resource_total(s, stride, lane, p);
    }
    return outside < 19u ? 19u - outside : 0u;
}
