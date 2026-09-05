// Mref v1: exact fixed-mass posterior, matching catan-core/src/dice.rs.
// The posterior is public evidence and is identical across hidden resource worlds.
// No controller is sampled into actor-facing state; every descendant conditions
// the complete posterior. Layout order matches ReferenceController's Ord.
typedef unsigned __int128 mref_u128;
typedef __int128 mref_i128;
#define MREF_MASS (1ull << 32)
#define MREF_PARTICLE_WORDS 28u
#define MREF_MAX_PARTICLES 64u
#define MREF_REMAINING 2u
#define MREF_CARDS 13u
#define MREF_RECENT 14u
#define MREF_RECENT_LEN 19u
#define MREF_INITIALIZED 20u
#define MREF_SEVENS 21u
#define MREF_OWNER 25u
#define MREF_STREAK 26u
#define MREF_PREPARED 27u
static const uint32_t MREF_DECK[11] = {1,2,3,4,5,6,5,4,3,2,1};

static inline __device__ uint32_t mref_get(const uint32_t *s, uint32_t stride,
    uint32_t lane, uint32_t particle, uint32_t field) {
    return state_get(s, stride, STATE_DICE_PARTICLES + particle * MREF_PARTICLE_WORDS + field, lane);
}
static inline __device__ void mref_set(uint32_t *s, uint32_t stride,
    uint32_t lane, uint32_t particle, uint32_t field, uint32_t value) {
    state_set(s, stride, STATE_DICE_PARTICLES + particle * MREF_PARTICLE_WORDS + field, lane, value);
}
static inline __device__ uint64_t mref_mass(const uint32_t *s, uint32_t stride,
    uint32_t lane, uint32_t p) {
    return (uint64_t)mref_get(s, stride, lane, p, 0)
        | ((uint64_t)mref_get(s, stride, lane, p, 1) << 32);
}
static inline __device__ void mref_set_mass(uint32_t *s, uint32_t stride,
    uint32_t lane, uint32_t p, uint64_t value) {
    mref_set(s, stride, lane, p, 0, (uint32_t)value);
    mref_set(s, stride, lane, p, 1, (uint32_t)(value >> 32));
}

// Largest remainder allocation. Ascending outcome index breaks ties exactly
// as on CPU. Common-denominator scaling does not change quotient/tie order.
static __device__ __noinline__ void mref_normalize(const mref_u128 *weights, uint64_t *out) {
    mref_u128 sum = 0;
    for (uint32_t i = 0; i < 11; ++i) sum += weights[i];
    mref_u128 remainder[11];
    uint64_t assigned = 0;
    for (uint32_t i = 0; i < 11; ++i) {
        const mref_u128 numerator = weights[i] * MREF_MASS;
        out[i] = sum ? (uint64_t)(numerator / sum) : 0;
        remainder[i] = sum ? numerator % sum : 0;
        assigned += out[i];
    }
    if (!sum) return;
    uint32_t used = 0;
    for (uint64_t n = assigned; n < MREF_MASS; ++n) {
        uint32_t best = 11;
        for (uint32_t i = 0; i < 11; ++i) {
            if (!(used & (1u << i)) && (best == 11 || remainder[i] > remainder[best])) best = i;
        }
        if (best == 11) return;
        ++out[best];
        used |= 1u << best;
    }
}

static __device__ __noinline__ void mref_controller_distribution(const uint32_t *s,
    uint32_t stride, uint32_t lane, uint32_t p, uint32_t actor, uint64_t *out) {
    for (uint32_t i = 0; i < 11; ++i) out[i] = 0;
    const uint32_t players = state_get(s, stride, STATE_NUM_PLAYERS, lane);
    const uint32_t initialized = mref_get(s, stride, lane, p, MREF_INITIALIZED);
    if (actor >= players || !(initialized & (1u << actor))
        || mref_get(s, stride, lane, p, MREF_PREPARED) != actor + 1u) return;
    uint32_t n = 0;
    for (uint32_t i = 0; i < players; ++i) n += (initialized >> i) & 1u;
    mref_i128 total = 0;
    for (uint32_t i = 0; i < players; ++i) total += mref_get(s, stride, lane, p, MREF_SEVENS + i);
    mref_i128 denominator = total < n ? 1 : total;
    mref_i128 numerator = total < n ? 1
        : 2 * total - (mref_i128)n * mref_get(s, stride, lane, p, MREF_SEVENS + actor);
    const mref_i128 streak = (mref_i128)2 * mref_get(s, stride, lane, p, MREF_STREAK);
    numerator = 5 * numerator + (mref_get(s, stride, lane, p, MREF_OWNER) == actor + 1u
        ? -streak : streak) * denominator;
    denominator *= 5;
    if (numerator < 0) numerator = 0;
    if (numerator > 2 * denominator) numerator = 2 * denominator;
    mref_u128 weights[11];
    const uint32_t recent_len = mref_get(s, stride, lane, p, MREF_RECENT_LEN);
    for (uint32_t i = 0; i < 11; ++i) {
        uint32_t recent_count = 0;
        for (uint32_t r = 0; r < recent_len; ++r) recent_count += mref_get(s, stride, lane, p, MREF_RECENT + r) == i + 2;
        const uint32_t suppression = recent_count >= 3 ? 0 : 100 - 34 * recent_count;
        const mref_u128 base = (mref_u128)mref_get(s, stride, lane, p, MREF_REMAINING + i) * suppression;
        weights[i] = base * (mref_u128)(i == 5 ? numerator : denominator);
    }
    mref_normalize(weights, out);
}

static __device__ __noinline__ void mref_distribution(const uint32_t *s, uint32_t stride,
    uint32_t lane, uint64_t *out) {
    mref_u128 weights[11] = {};
    const uint32_t count = state_get(s, stride, STATE_DICE_COUNT, lane);
    const uint32_t actor = state_get(s, stride, STATE_CURRENT_PLAYER, lane);
    for (uint32_t p = 0; p < count; ++p) {
        uint64_t law[11];
        mref_controller_distribution(s, stride, lane, p, actor, law);
        const uint64_t mass = mref_mass(s, stride, lane, p);
        for (uint32_t i = 0; i < 11; ++i) weights[i] += (mref_u128)mass * law[i];
    }
    mref_normalize(weights, out);
}

static inline __device__ int mref_compare(const uint32_t *s, uint32_t stride,
    uint32_t lane, uint32_t a, uint32_t b) {
    for (uint32_t f = MREF_REMAINING; f < MREF_PARTICLE_WORDS; ++f) {
        const uint32_t x = mref_get(s, stride, lane, a, f);
        const uint32_t y = mref_get(s, stride, lane, b, f);
        if (x != y) return x < y ? -1 : 1;
    }
    return 0;
}
static inline __device__ void mref_copy(uint32_t *s, uint32_t stride,
    uint32_t lane, uint32_t dst, uint32_t src) {
    if (dst == src) return;
    for (uint32_t f = 0; f < MREF_PARTICLE_WORDS; ++f) mref_set(s, stride, lane, dst, f, mref_get(s, stride, lane, src, f));
}

static __device__ __noinline__ bool mref_coalesce(uint32_t *s, uint32_t stride, uint32_t lane) {
    const uint32_t old_count = state_get(s, stride, STATE_DICE_COUNT, lane);
    uint32_t count = 0;
    for (uint32_t p = 0; p < old_count; ++p) {
        if (mref_mass(s, stride, lane, p)) mref_copy(s, stride, lane, count++, p);
    }
    if (!count) return false;
    // Sorting the small, bounded posterior preserves CPU canonical tie-breaking.
    for (uint32_t i = 0; i < count; ++i) {
        uint32_t best = i;
        for (uint32_t j = i + 1; j < count; ++j) if (mref_compare(s, stride, lane, j, best) < 0) best = j;
        if (best != i) for (uint32_t f = 0; f < MREF_PARTICLE_WORDS; ++f) {
            const uint32_t tmp = mref_get(s, stride, lane, i, f);
            mref_set(s, stride, lane, i, f, mref_get(s, stride, lane, best, f));
            mref_set(s, stride, lane, best, f, tmp);
        }
    }
    uint32_t coalesced = 0;
    for (uint32_t p = 0; p < count; ++p) {
        if (coalesced && mref_compare(s, stride, lane, coalesced - 1, p) == 0) {
            mref_set_mass(s, stride, lane, coalesced - 1,
                mref_mass(s, stride, lane, coalesced - 1) + mref_mass(s, stride, lane, p));
        } else mref_copy(s, stride, lane, coalesced++, p);
    }
    if (coalesced == 1) mref_set_mass(s, stride, lane, 0, MREF_MASS);
    else {
        uint64_t sum = 0;
        for (uint32_t p = 0; p < coalesced; ++p) sum += mref_mass(s, stride, lane, p);
        uint64_t remainder[MREF_MAX_PARTICLES];
        uint64_t assigned = 0;
        for (uint32_t p = 0; p < coalesced; ++p) {
            const mref_u128 scaled = (mref_u128)mref_mass(s, stride, lane, p) * MREF_MASS;
            const uint64_t mass = (uint64_t)(scaled / sum);
            remainder[p] = (uint64_t)(scaled % sum);
            mref_set_mass(s, stride, lane, p, mass);
            assigned += mass;
        }
        uint64_t used = 0;
        for (uint64_t n = assigned; n < MREF_MASS; ++n) {
            uint32_t best = coalesced;
            for (uint32_t p = 0; p < coalesced; ++p) {
                if (!(used & (1ull << p)) && (best == coalesced || remainder[p] > remainder[best])) best = p;
            }
            if (best == coalesced) return false;
            mref_set_mass(s, stride, lane, best, mref_mass(s, stride, lane, best) + 1);
            used |= 1ull << best;
        }
    }
    for (uint32_t p = coalesced; p < old_count; ++p)
        for (uint32_t f = 0; f < MREF_PARTICLE_WORDS; ++f) mref_set(s, stride, lane, p, f, 0);
    state_set(s, stride, STATE_DICE_COUNT, lane, coalesced);
    return true;
}

static __device__ __noinline__ bool mref_prepare(uint32_t *s, uint32_t stride, uint32_t lane) {
    const uint32_t count = state_get(s, stride, STATE_DICE_COUNT, lane);
    const uint32_t actor = state_get(s, stride, STATE_CURRENT_PLAYER, lane);
    if (!count || count > MREF_MAX_PARTICLES) return false;
    for (uint32_t p = 0; p < count; ++p) if (mref_get(s, stride, lane, p, MREF_PREPARED)) return false;
    for (uint32_t p = 0; p < count; ++p) {
        mref_set(s, stride, lane, p, MREF_INITIALIZED, mref_get(s, stride, lane, p, MREF_INITIALIZED) | (1u << actor));
        if (mref_get(s, stride, lane, p, MREF_CARDS) < 13) {
            for (uint32_t i = 0; i < 11; ++i) mref_set(s, stride, lane, p, MREF_REMAINING + i, MREF_DECK[i]);
            mref_set(s, stride, lane, p, MREF_CARDS, 36);
        }
        mref_set(s, stride, lane, p, MREF_PREPARED, actor + 1);
    }
    return mref_coalesce(s, stride, lane);
}
static inline __device__ uint32_t mref_increment(uint32_t value) {
    return value == 0xffffffffu ? value : value + 1;
}
static __device__ __noinline__ bool mref_resolve(uint32_t *s, uint32_t stride, uint32_t lane, uint32_t total) {
    const uint32_t count = state_get(s, stride, STATE_DICE_COUNT, lane);
    const uint32_t actor = state_get(s, stride, STATE_CURRENT_PLAYER, lane);
    if (!count || count > MREF_MAX_PARTICLES || total < 2 || total > 12) return false;
    uint64_t posterior[MREF_MAX_PARTICLES];
    uint64_t surviving = 0;
    for (uint32_t p = 0; p < count; ++p) {
        uint64_t law[11];
        mref_controller_distribution(s, stride, lane, p, actor, law);
        posterior[p] = (uint64_t)(((mref_u128)mref_mass(s, stride, lane, p) * law[total - 2]) >> 32);
        surviving += posterior[p];
    }
    if (!surviving) return false;
    for (uint32_t p = 0; p < count; ++p) {
        mref_set_mass(s, stride, lane, p, posterior[p]);
        if (!posterior[p]) continue;
        mref_set(s, stride, lane, p, MREF_REMAINING + total - 2, mref_get(s, stride, lane, p, MREF_REMAINING + total - 2) - 1);
        mref_set(s, stride, lane, p, MREF_CARDS, mref_get(s, stride, lane, p, MREF_CARDS) - 1);
        uint32_t len = mref_get(s, stride, lane, p, MREF_RECENT_LEN);
        if (len == 5) {
            for (uint32_t r = 0; r < 4; ++r) mref_set(s, stride, lane, p, MREF_RECENT + r, mref_get(s, stride, lane, p, MREF_RECENT + r + 1));
            mref_set(s, stride, lane, p, MREF_RECENT + 4, total);
        } else {
            mref_set(s, stride, lane, p, MREF_RECENT + len, total);
            mref_set(s, stride, lane, p, MREF_RECENT_LEN, len + 1);
        }
        if (total == 7) {
            mref_set(s, stride, lane, p, MREF_SEVENS + actor, mref_increment(mref_get(s, stride, lane, p, MREF_SEVENS + actor)));
            const uint32_t streak = mref_get(s, stride, lane, p, MREF_OWNER) == actor + 1
                ? mref_increment(mref_get(s, stride, lane, p, MREF_STREAK)) : 1;
            mref_set(s, stride, lane, p, MREF_OWNER, actor + 1);
            mref_set(s, stride, lane, p, MREF_STREAK, streak);
        }
        mref_set(s, stride, lane, p, MREF_PREPARED, 0);
    }
    return mref_coalesce(s, stride, lane);
}

// Exact vectors exposed for semantic parity, not an alternative game model.
extern "C" __global__ void dice_distribution_kernel(const uint32_t *s,
    uint32_t stride, uint64_t *out, uint32_t count) {
    const uint32_t lane = blockIdx.x * blockDim.x + threadIdx.x;
    if (lane >= count) return;
    uint64_t law[11] = {};
    if (state_get(s, stride, STATE_PHASE, lane) == PHASE_ROLL_CHANCE) {
        if (state_get(s, stride, STATE_DICE_MODEL, lane)) mref_distribution(s, stride, lane, law);
        else for (uint32_t i = 0; i < 11; ++i) law[i] = MREF_DECK[i];
    }
    for (uint32_t i = 0; i < 11; ++i) out[i * stride + lane] = law[i];
}
