use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use crate::SplitMix64;

pub const M0_FAIR_IID_2D6_V1: &str = "m0-fair-iid-2d6-v1";
pub const MREF_COLONIST_LINKED_2024_V1: &str = "mref-colonist-linked-2024-v1";
pub const PUBLIC_HISTORY_BELIEF_V1: &str = "public-history-belief-v1";
pub const FIXED_BELIEF_MASS: u64 = 1u64 << 32;
pub const REFERENCE_PARTICLES: usize = 64;
pub const TOTAL_MIN: u8 = 2;
pub const TOTAL_MAX: u8 = 12;
pub const TOTAL_COUNT: usize = 11;
pub const REFERENCE_DECK_COUNTS: [u8; TOTAL_COUNT] = [1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StochasticModel {
    #[default]
    M0FairIid2d6V1,
    MrefColonistLinked2024V1,
}

impl StochasticModel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::M0FairIid2d6V1 => M0_FAIR_IID_2D6_V1,
            Self::MrefColonistLinked2024V1 => MREF_COLONIST_LINKED_2024_V1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BeliefPolicy {
    PublicHistoryV1,
}

impl BeliefPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PublicHistoryV1 => PUBLIC_HISTORY_BELIEF_V1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PublicRollObservation {
    pub ordinal: u32,
    pub actor: u8,
    pub total: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MissingRollGap {
    pub after_ordinal: u32,
    pub missing_rolls: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiceHistoryProvenance {
    CompleteFromFirstGameplayRoll,
    GapFreeSuffix {
        missing_prefix_rolls: Option<u32>,
    },
    Gapped {
        missing_prefix_rolls: Option<u32>,
        gaps: Vec<MissingRollGap>,
    },
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StochasticHistoryError {
    InvalidPlayerCount,
    InvalidActor(u8),
    InvalidTotal(u8),
    Unavailable(&'static str),
    Inconsistent(String),
}

impl fmt::Display for StochasticHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlayerCount => {
                write!(formatter, "stochastic history has invalid player count")
            }
            Self::InvalidActor(actor) => {
                write!(formatter, "stochastic history has invalid actor {actor}")
            }
            Self::InvalidTotal(total) => {
                write!(formatter, "stochastic history has invalid total {total}")
            }
            Self::Unavailable(reason) => write!(
                formatter,
                "reference stochastic history unavailable: {reason}"
            ),
            Self::Inconsistent(reason) => write!(
                formatter,
                "reference stochastic history inconsistent: {reason}"
            ),
        }
    }
}

impl std::error::Error for StochasticHistoryError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ratio {
    num: i128,
    den: i128,
}

impl Ratio {
    const ZERO: Self = Self { num: 0, den: 1 };
    const ONE: Self = Self { num: 1, den: 1 };
    const TWO: Self = Self { num: 2, den: 1 };

    fn new(num: i128, den: i128) -> Self {
        assert!(den != 0);
        if num == 0 {
            return Self::ZERO;
        }
        let mut num = num;
        let mut den = den;
        if den < 0 {
            num = -num;
            den = -den;
        }
        let divisor = gcd_i128(num.unsigned_abs(), den as u128) as i128;
        Self {
            num: num / divisor,
            den: den / divisor,
        }
    }

    fn add(self, other: Self) -> Self {
        Self::new(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    fn mul(self, other: Self) -> Self {
        Self::new(self.num * other.num, self.den * other.den)
    }

    fn clamp(self, minimum: Self, maximum: Self) -> Self {
        if self.cmp(minimum) == Ordering::Less {
            minimum
        } else if self.cmp(maximum) == Ordering::Greater {
            maximum
        } else {
            self
        }
    }

    fn cmp(self, other: Self) -> Ordering {
        (self.num * other.den).cmp(&(other.num * self.den))
    }

    fn nonnegative_parts(self) -> (u128, u128) {
        if self.num <= 0 {
            (0, 1)
        } else {
            (self.num as u128, self.den as u128)
        }
    }
}

fn gcd_i128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left.max(1)
}

fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left.max(1)
}

fn lcm_u128(left: u128, right: u128) -> u128 {
    left / gcd_u128(left, right) * right
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReferenceController {
    num_players: u8,
    remaining: [u8; TOTAL_COUNT],
    cards_left: u8,
    recent: [u8; 5],
    recent_len: u8,
    initialized_players: u8,
    seven_counts: [u32; 4],
    seven_streak_owner: Option<u8>,
    seven_streak_count: u32,
    prepared_actor: Option<u8>,
}

impl ReferenceController {
    pub fn new(num_players: u8) -> Result<Self, StochasticHistoryError> {
        if !(2..=4).contains(&num_players) {
            return Err(StochasticHistoryError::InvalidPlayerCount);
        }
        Ok(Self {
            num_players,
            remaining: REFERENCE_DECK_COUNTS,
            cards_left: 36,
            recent: [0; 5],
            recent_len: 0,
            initialized_players: 0,
            seven_counts: [0; 4],
            seven_streak_owner: None,
            seven_streak_count: 0,
            prepared_actor: None,
        })
    }

    pub const fn cards_left(&self) -> u8 {
        self.cards_left
    }

    pub const fn remaining_counts(&self) -> [u8; TOTAL_COUNT] {
        self.remaining
    }

    pub fn recent_totals(&self) -> Vec<u8> {
        self.recent[..self.recent_len as usize].to_vec()
    }

    pub const fn initialized_player_mask(&self) -> u8 {
        self.initialized_players
    }

    pub const fn seven_counts(&self) -> [u32; 4] {
        self.seven_counts
    }

    pub const fn seven_streak_owner(&self) -> Option<u8> {
        self.seven_streak_owner
    }

    pub const fn seven_streak_count(&self) -> u32 {
        self.seven_streak_count
    }

    pub const fn prepared_actor(&self) -> Option<u8> {
        self.prepared_actor
    }

    pub fn prepare_roll(&mut self, actor: u8) -> Result<(), StochasticHistoryError> {
        self.validate_actor(actor)?;
        if self.prepared_actor.is_some() {
            return Err(StochasticHistoryError::Inconsistent(
                "controller already has a prepared roll".into(),
            ));
        }
        self.initialized_players |= 1 << actor;
        if self.cards_left < 13 {
            self.remaining = REFERENCE_DECK_COUNTS;
            self.cards_left = 36;
        }
        self.prepared_actor = Some(actor);
        Ok(())
    }

    pub fn outcome_weight(&self, actor: u8, total: u8) -> u64 {
        if !(TOTAL_MIN..=TOTAL_MAX).contains(&total) || self.prepared_actor != Some(actor) {
            return 0;
        }
        self.fixed_distribution(actor)[(total - TOTAL_MIN) as usize]
    }

    pub fn seven_adjustment_parts(&self, actor: u8) -> (u64, u64) {
        let ratio = self.seven_adjustment(actor);
        let (num, den) = ratio.nonnegative_parts();
        (num as u64, den as u64)
    }

    pub fn fixed_distribution(&self, actor: u8) -> [u64; TOTAL_COUNT] {
        if self.prepared_actor != Some(actor) || actor >= self.num_players {
            return [0; TOTAL_COUNT];
        }
        let weights = (TOTAL_MIN..=TOTAL_MAX)
            .map(|total| self.raw_weight(actor, total))
            .collect::<Vec<_>>();
        normalize_ratios(&weights)
    }

    pub fn resolve_roll(&mut self, actor: u8, total: u8) -> Result<(), StochasticHistoryError> {
        self.validate_actor(actor)?;
        self.validate_total(total)?;
        if self.prepared_actor != Some(actor) {
            return Err(StochasticHistoryError::Inconsistent(
                "roll resolved without matching prepared actor".into(),
            ));
        }
        if self.outcome_weight(actor, total) == 0 {
            return Err(StochasticHistoryError::Inconsistent(format!(
                "observed roll {total} has zero reference probability"
            )));
        }
        let index = (total - TOTAL_MIN) as usize;
        if self.remaining[index] == 0 || self.cards_left == 0 {
            return Err(StochasticHistoryError::Inconsistent(format!(
                "observed roll {total} is absent from the reference deck"
            )));
        }
        self.remaining[index] -= 1;
        self.cards_left -= 1;
        self.push_recent(total);
        if total == 7 {
            self.seven_counts[actor as usize] = self.seven_counts[actor as usize].saturating_add(1);
            if self.seven_streak_owner == Some(actor) {
                self.seven_streak_count = self.seven_streak_count.saturating_add(1);
            } else {
                self.seven_streak_owner = Some(actor);
                self.seven_streak_count = 1;
            }
        }
        self.prepared_actor = None;
        Ok(())
    }

    fn validate_actor(&self, actor: u8) -> Result<(), StochasticHistoryError> {
        if actor >= self.num_players {
            Err(StochasticHistoryError::InvalidActor(actor))
        } else {
            Ok(())
        }
    }

    fn validate_total(&self, total: u8) -> Result<(), StochasticHistoryError> {
        if !(TOTAL_MIN..=TOTAL_MAX).contains(&total) {
            Err(StochasticHistoryError::InvalidTotal(total))
        } else {
            Ok(())
        }
    }

    fn push_recent(&mut self, total: u8) {
        let len = self.recent_len as usize;
        if len < self.recent.len() {
            self.recent[len] = total;
            self.recent_len += 1;
            return;
        }
        self.recent.copy_within(1.., 0);
        self.recent[4] = total;
    }

    fn recent_count(&self, total: u8) -> u8 {
        self.recent[..self.recent_len as usize]
            .iter()
            .filter(|value| **value == total)
            .count() as u8
    }

    fn raw_weight(&self, actor: u8, total: u8) -> Ratio {
        let index = (total - TOTAL_MIN) as usize;
        let remaining = self.remaining[index] as i128;
        if remaining == 0 {
            return Ratio::ZERO;
        }
        let recent_count = self.recent_count(total) as i128;
        let recent_numerator = (100 - 34 * recent_count).max(0);
        if recent_numerator == 0 {
            return Ratio::ZERO;
        }
        let mut result = Ratio::new(remaining * recent_numerator, 100);
        if total == 7 {
            result = result.mul(self.seven_adjustment(actor));
        }
        result
    }

    fn seven_adjustment(&self, actor: u8) -> Ratio {
        if actor >= self.num_players || self.initialized_players & (1 << actor) == 0 {
            return Ratio::ZERO;
        }
        let initialized = self.initialized_players.count_ones() as i128;
        let total_sevens = self.seven_counts[..self.num_players as usize]
            .iter()
            .map(|count| *count as i128)
            .sum::<i128>();
        let actor_sevens = self.seven_counts[actor as usize] as i128;
        let imbalance = if total_sevens < initialized {
            Ratio::ONE
        } else {
            Ratio::new(2 * total_sevens - initialized * actor_sevens, total_sevens)
        };
        let streak_magnitude = Ratio::new(2 * self.seven_streak_count as i128, 5);
        let streak = if self.seven_streak_owner == Some(actor) {
            Ratio::new(-streak_magnitude.num, streak_magnitude.den)
        } else {
            streak_magnitude
        };
        imbalance.add(streak).clamp(Ratio::ZERO, Ratio::TWO)
    }

    fn digest_into(&self, hash: &mut u64) {
        fn byte(hash: &mut u64, value: u8) {
            *hash ^= value as u64;
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        byte(hash, self.num_players);
        for value in self.remaining {
            byte(hash, value);
        }
        byte(hash, self.cards_left);
        byte(hash, self.recent_len);
        for value in self.recent {
            byte(hash, value);
        }
        byte(hash, self.initialized_players);
        for count in self.seven_counts {
            for value in count.to_le_bytes() {
                byte(hash, value);
            }
        }
        byte(
            hash,
            self.seven_streak_owner.map(|actor| actor + 1).unwrap_or(0),
        );
        for value in self.seven_streak_count.to_le_bytes() {
            byte(hash, value);
        }
        byte(
            hash,
            self.prepared_actor.map(|actor| actor + 1).unwrap_or(0),
        );
    }
}

fn normalize_ratios(weights: &[Ratio]) -> [u64; TOTAL_COUNT] {
    let mut denominator = 1u128;
    for weight in weights {
        let (_, den) = weight.nonnegative_parts();
        denominator = lcm_u128(denominator, den);
    }
    let scaled = weights
        .iter()
        .map(|weight| {
            let (num, den) = weight.nonnegative_parts();
            num.saturating_mul(denominator / den)
        })
        .collect::<Vec<_>>();
    let total = scaled.iter().sum::<u128>();
    if total == 0 {
        return [0; TOTAL_COUNT];
    }
    let mut result = [0u64; TOTAL_COUNT];
    let mut remainders = Vec::with_capacity(TOTAL_COUNT);
    let mut assigned = 0u64;
    for (index, value) in scaled.iter().copied().enumerate() {
        let numerator = value * FIXED_BELIEF_MASS as u128;
        let quotient = (numerator / total) as u64;
        let remainder = numerator % total;
        result[index] = quotient;
        assigned += quotient;
        remainders.push((remainder, index));
    }
    remainders.sort_by(
        |(left_remainder, left_index), (right_remainder, right_index)| {
            right_remainder
                .cmp(left_remainder)
                .then_with(|| left_index.cmp(right_index))
        },
    );
    for (_, index) in remainders
        .into_iter()
        .take((FIXED_BELIEF_MASS - assigned) as usize)
    {
        result[index] += 1;
    }
    result
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ControllerParticle {
    pub mass: u64,
    pub controller: ReferenceController,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StochasticBelief {
    num_players: u8,
    particles: Vec<ControllerParticle>,
}

impl StochasticBelief {
    pub fn from_public_history(
        num_players: u8,
        observations: &[PublicRollObservation],
        provenance: &DiceHistoryProvenance,
        construction_seed: u64,
    ) -> Result<Self, StochasticHistoryError> {
        if !(2..=4).contains(&num_players) {
            return Err(StochasticHistoryError::InvalidPlayerCount);
        }
        validate_observations(num_players, observations)?;
        match provenance {
            DiceHistoryProvenance::CompleteFromFirstGameplayRoll => {
                let mut controller = ReferenceController::new(num_players)?;
                for (expected_ordinal, observation) in observations.iter().enumerate() {
                    if observation.ordinal != expected_ordinal as u32 {
                        return Err(StochasticHistoryError::Inconsistent(
                            "complete history contains a missing roll ordinal".into(),
                        ));
                    }
                    controller.prepare_roll(observation.actor)?;
                    controller.resolve_roll(observation.actor, observation.total)?;
                }
                Ok(Self {
                    num_players,
                    particles: vec![ControllerParticle {
                        mass: FIXED_BELIEF_MASS,
                        controller,
                    }],
                })
            }
            DiceHistoryProvenance::GapFreeSuffix {
                missing_prefix_rolls,
            } => {
                let missing_prefix_rolls =
                    missing_prefix_rolls.ok_or(StochasticHistoryError::Unavailable(
                        "gap-free suffix does not establish the missing prefix roll count",
                    ))?;
                Self::partial_from_history(
                    num_players,
                    observations,
                    missing_prefix_rolls,
                    &[],
                    construction_seed,
                )
            }
            DiceHistoryProvenance::Gapped {
                missing_prefix_rolls,
                gaps,
            } => {
                if gaps.iter().any(|gap| gap.missing_rolls.is_none()) {
                    return Err(StochasticHistoryError::Unavailable(
                        "dice history contains a gap with unknown roll length",
                    ));
                }
                Self::partial_from_history(
                    num_players,
                    observations,
                    missing_prefix_rolls.unwrap_or(0),
                    gaps,
                    construction_seed,
                )
            }
            DiceHistoryProvenance::Unknown => Err(StochasticHistoryError::Unavailable(
                "dice history provenance is unknown",
            )),
        }
    }

    fn partial_from_history(
        num_players: u8,
        observations: &[PublicRollObservation],
        missing_prefix_rolls: u32,
        gaps: &[MissingRollGap],
        construction_seed: u64,
    ) -> Result<Self, StochasticHistoryError> {
        let first = observations
            .first()
            .ok_or(StochasticHistoryError::Unavailable(
                "partial history has no observed roll to anchor actor order",
            ))?;
        if first.ordinal != missing_prefix_rolls {
            return Err(StochasticHistoryError::Inconsistent(format!(
                "first observed ordinal {} does not match missing prefix count {missing_prefix_rolls}",
                first.ordinal
            )));
        }
        for gap in gaps {
            let missing = gap.missing_rolls.unwrap_or(0);
            let next_ordinal = gap.after_ordinal.saturating_add(missing).saturating_add(1);
            if !observations
                .iter()
                .any(|observation| observation.ordinal == next_ordinal)
            {
                return Err(StochasticHistoryError::Inconsistent(format!(
                    "declared roll gap after ordinal {} is not anchored by ordinal {next_ordinal}",
                    gap.after_ordinal
                )));
            }
        }
        let base_mass = FIXED_BELIEF_MASS / REFERENCE_PARTICLES as u64;
        let mut particles = Vec::with_capacity(REFERENCE_PARTICLES);
        for particle_index in 0..REFERENCE_PARTICLES {
            let controller = ReferenceController::new(num_players)?;
            particles.push(ControllerParticle {
                mass: base_mass,
                controller,
            });
            let _ = particle_index;
        }
        let mut belief = Self {
            num_players,
            particles,
        };
        for particle_index in 0..belief.particles.len() {
            let seed = mix_seed(construction_seed, particle_index as u64);
            let mut rng = SplitMix64::new(seed);
            let prefix_start_actor = rewind_actor(first.actor, missing_prefix_rolls, num_players);
            for offset in 0..missing_prefix_rolls {
                let actor =
                    (prefix_start_actor + (offset % num_players as u32) as u8) % num_players;
                sample_missing_roll(
                    &mut belief.particles[particle_index].controller,
                    actor,
                    &mut rng,
                )?;
            }
        }
        belief.condition_observations(observations, construction_seed)?;
        belief.coalesce_and_normalize()?;
        Ok(belief)
    }

    fn condition_observations(
        &mut self,
        observations: &[PublicRollObservation],
        construction_seed: u64,
    ) -> Result<(), StochasticHistoryError> {
        let mut previous: Option<PublicRollObservation> = None;
        for observation in observations {
            if let Some(previous) = previous {
                if observation.ordinal <= previous.ordinal {
                    return Err(StochasticHistoryError::Inconsistent(
                        "roll observations are not strictly ordered".into(),
                    ));
                }
                let missing = observation.ordinal - previous.ordinal - 1;
                for particle_index in 0..self.particles.len() {
                    if self.particles[particle_index].mass == 0 {
                        continue;
                    }
                    let mut rng = SplitMix64::new(mix_seed(
                        construction_seed ^ observation.ordinal as u64,
                        particle_index as u64,
                    ));
                    for offset in 0..missing {
                        let actor = (previous.actor + 1 + (offset % self.num_players as u32) as u8)
                            % self.num_players;
                        sample_missing_roll(
                            &mut self.particles[particle_index].controller,
                            actor,
                            &mut rng,
                        )?;
                    }
                }
                let expected_actor =
                    (previous.actor + (missing as u8 % self.num_players) + 1) % self.num_players;
                if observation.actor != expected_actor {
                    return Err(StochasticHistoryError::Inconsistent(format!(
                        "actor {} does not follow actor {} across {} missing rolls",
                        observation.actor, previous.actor, missing
                    )));
                }
            }
            self.condition_exact_roll(*observation)?;
            previous = Some(*observation);
        }
        Ok(())
    }

    fn condition_exact_roll(
        &mut self,
        observation: PublicRollObservation,
    ) -> Result<(), StochasticHistoryError> {
        for particle in &mut self.particles {
            if particle.mass == 0 {
                continue;
            }
            particle.controller.prepare_roll(observation.actor)?;
            let probability = particle
                .controller
                .outcome_weight(observation.actor, observation.total);
            if probability == 0 {
                particle.mass = 0;
                particle.controller.prepared_actor = None;
                continue;
            }
            particle.mass =
                ((particle.mass as u128 * probability as u128) / FIXED_BELIEF_MASS as u128) as u64;
            if particle.mass > 0 {
                particle
                    .controller
                    .resolve_roll(observation.actor, observation.total)?;
            } else {
                particle.controller.prepared_actor = None;
            }
        }
        // Preserve the 64 deterministic generative trajectories through all
        // missing intervals. Equivalent controllers are coalesced only after
        // the full public history has been conditioned, so future gap draws do
        // not collapse independent particle streams prematurely.
        normalize_particle_masses(&mut self.particles)
    }

    pub fn prepare_roll(&self, actor: u8) -> Result<Self, StochasticHistoryError> {
        let mut next = self.clone();
        for particle in &mut next.particles {
            particle.controller.prepare_roll(actor)?;
        }
        next.coalesce_and_normalize()?;
        Ok(next)
    }

    pub fn condition_and_resolve(
        &self,
        actor: u8,
        total: u8,
    ) -> Result<Self, StochasticHistoryError> {
        let mut next = self.clone();
        for particle in &mut next.particles {
            let probability = particle.controller.outcome_weight(actor, total);
            if probability == 0 {
                particle.mass = 0;
                particle.controller.prepared_actor = None;
                continue;
            }
            particle.mass =
                ((particle.mass as u128 * probability as u128) / FIXED_BELIEF_MASS as u128) as u64;
            if particle.mass > 0 {
                particle.controller.resolve_roll(actor, total)?;
            } else {
                particle.controller.prepared_actor = None;
            }
        }
        next.coalesce_and_normalize()?;
        Ok(next)
    }

    pub fn distribution(&self, actor: u8) -> [u64; TOTAL_COUNT] {
        let mut numerators = [0u128; TOTAL_COUNT];
        for particle in &self.particles {
            let distribution = particle.controller.fixed_distribution(actor);
            for (index, probability) in distribution.into_iter().enumerate() {
                numerators[index] =
                    numerators[index].saturating_add(particle.mass as u128 * probability as u128);
            }
        }
        normalize_fixed_numerators(numerators)
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    pub fn particles(&self) -> &[ControllerParticle] {
        &self.particles
    }

    pub fn total_mass(&self) -> u64 {
        self.particles.iter().map(|particle| particle.mass).sum()
    }

    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325;
        fn byte(hash: &mut u64, value: u8) {
            *hash ^= value as u64;
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        byte(&mut hash, self.num_players);
        for particle in &self.particles {
            for value in particle.mass.to_le_bytes() {
                byte(&mut hash, value);
            }
            particle.controller.digest_into(&mut hash);
        }
        hash
    }

    fn coalesce_and_normalize(&mut self) -> Result<(), StochasticHistoryError> {
        self.particles.retain(|particle| particle.mass > 0);
        if self.particles.is_empty() {
            return Err(StochasticHistoryError::Inconsistent(
                "all stochastic particles lost mass".into(),
            ));
        }
        self.particles
            .sort_by(|left, right| left.controller.cmp(&right.controller));
        let mut coalesced: Vec<ControllerParticle> = Vec::with_capacity(self.particles.len());
        for particle in self.particles.drain(..) {
            if let Some(previous) = coalesced.last_mut()
                && previous.controller == particle.controller
            {
                previous.mass = previous.mass.saturating_add(particle.mass);
            } else {
                coalesced.push(particle);
            }
        }
        normalize_particle_masses(&mut coalesced)?;
        coalesced.sort_by(|left, right| left.controller.cmp(&right.controller));
        self.particles = coalesced;
        Ok(())
    }
}

fn validate_observations(
    num_players: u8,
    observations: &[PublicRollObservation],
) -> Result<(), StochasticHistoryError> {
    let mut previous_ordinal = None;
    for observation in observations {
        if observation.actor >= num_players {
            return Err(StochasticHistoryError::InvalidActor(observation.actor));
        }
        if !(TOTAL_MIN..=TOTAL_MAX).contains(&observation.total) {
            return Err(StochasticHistoryError::InvalidTotal(observation.total));
        }
        if previous_ordinal.is_some_and(|previous| observation.ordinal <= previous) {
            return Err(StochasticHistoryError::Inconsistent(
                "roll observations are not strictly ordered".into(),
            ));
        }
        previous_ordinal = Some(observation.ordinal);
    }
    Ok(())
}

fn sample_missing_roll(
    controller: &mut ReferenceController,
    actor: u8,
    rng: &mut SplitMix64,
) -> Result<(), StochasticHistoryError> {
    controller.prepare_roll(actor)?;
    let distribution = controller.fixed_distribution(actor);
    let total = distribution.iter().sum::<u64>();
    if total == 0 {
        return Err(StochasticHistoryError::Inconsistent(
            "missing-roll generator reached a zero-mass controller".into(),
        ));
    }
    let mut target = rng.next_u64() % total;
    let mut selected = TOTAL_MIN;
    for (index, weight) in distribution.into_iter().enumerate() {
        if target < weight {
            selected = TOTAL_MIN + index as u8;
            break;
        }
        target -= weight;
    }
    controller.resolve_roll(actor, selected)
}

fn rewind_actor(actor: u8, rolls: u32, num_players: u8) -> u8 {
    let offset = (rolls % num_players as u32) as u8;
    (actor + num_players - offset) % num_players
}

fn mix_seed(seed: u64, stream: u64) -> u64 {
    let mut value = seed ^ stream.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn normalize_particle_masses(
    particles: &mut [ControllerParticle],
) -> Result<(), StochasticHistoryError> {
    let total = particles
        .iter()
        .map(|particle| particle.mass as u128)
        .sum::<u128>();
    if total == 0 {
        return Err(StochasticHistoryError::Inconsistent(
            "all stochastic particles lost mass".into(),
        ));
    }
    let mut assigned = 0u64;
    let mut remainders = Vec::with_capacity(particles.len());
    for (index, particle) in particles.iter_mut().enumerate() {
        let numerator = particle.mass as u128 * FIXED_BELIEF_MASS as u128;
        let quotient = (numerator / total) as u64;
        let remainder = numerator % total;
        particle.mass = quotient;
        assigned += quotient;
        remainders.push((remainder, index));
    }
    remainders.sort_by(
        |(left_remainder, left_index), (right_remainder, right_index)| {
            right_remainder.cmp(left_remainder).then_with(|| {
                particles[*left_index]
                    .controller
                    .cmp(&particles[*right_index].controller)
            })
        },
    );
    for (_, index) in remainders
        .into_iter()
        .take((FIXED_BELIEF_MASS - assigned) as usize)
    {
        particles[index].mass += 1;
    }
    Ok(())
}

fn normalize_fixed_numerators(numerators: [u128; TOTAL_COUNT]) -> [u64; TOTAL_COUNT] {
    let total = numerators.iter().sum::<u128>();
    if total == 0 {
        return [0; TOTAL_COUNT];
    }
    let mut result = [0u64; TOTAL_COUNT];
    let mut assigned = 0u64;
    let mut remainders = Vec::with_capacity(TOTAL_COUNT);
    for (index, value) in numerators.into_iter().enumerate() {
        let quotient = (value / FIXED_BELIEF_MASS as u128) as u64;
        let remainder = value % FIXED_BELIEF_MASS as u128;
        result[index] = quotient;
        assigned = assigned.saturating_add(quotient);
        remainders.push((remainder, index));
    }
    if assigned > FIXED_BELIEF_MASS {
        // This cannot occur for normalized particle/controller masses, but a
        // defensive exact renormalization keeps the API deterministic.
        let scaled = result.map(|value| value as u128);
        return normalize_u128_weights(scaled);
    }
    remainders.sort_by(
        |(left_remainder, left_index), (right_remainder, right_index)| {
            right_remainder
                .cmp(left_remainder)
                .then_with(|| left_index.cmp(right_index))
        },
    );
    for (_, index) in remainders
        .into_iter()
        .take((FIXED_BELIEF_MASS - assigned) as usize)
    {
        result[index] += 1;
    }
    result
}

fn normalize_u128_weights(weights: [u128; TOTAL_COUNT]) -> [u64; TOTAL_COUNT] {
    let total = weights.iter().sum::<u128>();
    if total == 0 {
        return [0; TOTAL_COUNT];
    }
    let mut result = [0u64; TOTAL_COUNT];
    let mut assigned = 0u64;
    let mut remainders = Vec::with_capacity(TOTAL_COUNT);
    for (index, value) in weights.into_iter().enumerate() {
        let numerator = value.saturating_mul(FIXED_BELIEF_MASS as u128);
        let quotient = (numerator / total) as u64;
        result[index] = quotient;
        assigned += quotient;
        remainders.push((numerator % total, index));
    }
    remainders.sort_by(
        |(left_remainder, left_index), (right_remainder, right_index)| {
            right_remainder
                .cmp(left_remainder)
                .then_with(|| left_index.cmp(right_index))
        },
    );
    for (_, index) in remainders
        .into_iter()
        .take((FIXED_BELIEF_MASS - assigned) as usize)
    {
        result[index] += 1;
    }
    result
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum StochasticState {
    #[default]
    M0,
    Reference {
        policy: BeliefPolicy,
        belief: Arc<StochasticBelief>,
    },
}

impl StochasticState {
    pub const fn m0() -> Self {
        Self::M0
    }

    pub fn reference(belief: StochasticBelief) -> Self {
        Self::Reference {
            policy: BeliefPolicy::PublicHistoryV1,
            belief: Arc::new(belief),
        }
    }

    pub const fn model(&self) -> StochasticModel {
        match self {
            Self::M0 => StochasticModel::M0FairIid2d6V1,
            Self::Reference { .. } => StochasticModel::MrefColonistLinked2024V1,
        }
    }

    pub const fn belief_policy(&self) -> Option<BeliefPolicy> {
        match self {
            Self::M0 => None,
            Self::Reference { policy, .. } => Some(*policy),
        }
    }

    pub fn digest(&self) -> u64 {
        match self {
            Self::M0 => 0,
            Self::Reference { policy, belief } => {
                let mut value = belief.digest();
                value ^= match policy {
                    BeliefPolicy::PublicHistoryV1 => 0x7068_6276_3100_0001,
                };
                value
            }
        }
    }

    pub fn particle_count(&self) -> usize {
        match self {
            Self::M0 => 1,
            Self::Reference { belief, .. } => belief.particle_count(),
        }
    }

    pub fn reference_belief(&self) -> Option<&StochasticBelief> {
        match self {
            Self::M0 => None,
            Self::Reference { belief, .. } => Some(belief.as_ref()),
        }
    }

    pub fn prepare_roll(&mut self, actor: u8) -> Result<(), StochasticHistoryError> {
        if let Self::Reference { policy, belief } = self {
            *belief = Arc::new(belief.prepare_roll(actor)?);
            let _ = policy;
        }
        Ok(())
    }

    pub fn resolve_roll(&mut self, actor: u8, total: u8) -> Result<(), StochasticHistoryError> {
        if let Self::Reference { policy, belief } = self {
            *belief = Arc::new(belief.condition_and_resolve(actor, total)?);
            let _ = policy;
        }
        Ok(())
    }

    pub fn reference_distribution(&self, actor: u8) -> Option<[u64; TOTAL_COUNT]> {
        match self {
            Self::M0 => None,
            Self::Reference { belief, .. } => Some(belief.distribution(actor)),
        }
    }
}
