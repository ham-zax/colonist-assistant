use std::fmt;

pub type ResourceHand = [u8; 5];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Resource {
    Lumber = 0,
    Brick = 1,
    Wool = 2,
    Grain = 3,
    Ore = 4,
}

impl Resource {
    pub const ALL: [Self; 5] = [
        Self::Lumber,
        Self::Brick,
        Self::Wool,
        Self::Grain,
        Self::Ore,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DevCard {
    Knight = 0,
    VictoryPoint = 1,
    RoadBuilding = 2,
    YearOfPlenty = 3,
    Monopoly = 4,
}

impl DevCard {
    pub const ALL: [Self; 5] = [
        Self::Knight,
        Self::VictoryPoint,
        Self::RoadBuilding,
        Self::YearOfPlenty,
        Self::Monopoly,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Port {
    Generic,
    Resource(Resource),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Building {
    Settlement(u8),
    City(u8),
}

impl Building {
    pub const fn player(self) -> u8 {
        match self {
            Self::Settlement(player) | Self::City(player) => player,
        }
    }

    pub const fn production_multiplier(self) -> u8 {
        match self {
            Self::Settlement(_) => 1,
            Self::City(_) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    SetupSettlement,
    SetupRoad { settlement: u8 },
    PreRoll,
    RollChance,
    Discard,
    MoveRobber,
    ResolveSteal { victim: u8 },
    Main,
    DevelopmentChance,
    TradeResponses,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TradeOffer {
    pub creator: u8,
    pub recipients: u8,
    pub give: ResourceHand,
    pub receive: ResourceHand,
    pub accepted: u8,
    pub rejected: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Decision { actor: u8 },
    Chance,
    Terminal,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    PlaceSettlement {
        vertex: u8,
    },
    PlaceRoad {
        edge: u8,
    },
    Roll,
    ResolveRoll {
        value: u8,
    },
    Discard {
        cards: ResourceHand,
    },
    MoveRobber {
        hex: u8,
        victim: Option<u8>,
    },
    ResolveSteal {
        victim: u8,
        resource: Resource,
    },
    BuildRoad {
        edge: u8,
    },
    BuildSettlement {
        vertex: u8,
    },
    BuildCity {
        vertex: u8,
    },
    BuyDevelopment,
    ResolveDevelopment {
        card: DevCard,
    },
    PlayKnight {
        hex: u8,
        victim: Option<u8>,
    },
    PlayRoadBuilding {
        first: u8,
        second: Option<u8>,
    },
    PlayYearOfPlenty {
        first: Resource,
        second: Resource,
    },
    PlayMonopoly {
        resource: Resource,
    },
    MaritimeTrade {
        give: Resource,
        receive: Resource,
        ratio: u8,
    },
    OfferTrade {
        recipients: u8,
        give: ResourceHand,
        receive: ResourceHand,
    },
    RespondTrade {
        accept: bool,
    },
    CounterTrade {
        give: ResourceHand,
        receive: ResourceHand,
    },
    ConfirmTrade {
        partner: u8,
    },
    CancelTrade,
    EndTurn,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerState {
    pub resources: ResourceHand,
    pub development: [u8; 5],
    pub bought_development: [u8; 5],
    pub public_victory_points: u8,
    pub played_knights: u8,
    pub roads_left: u8,
    pub settlements_left: u8,
    pub cities_left: u8,
    pub has_longest_road: bool,
    pub has_largest_army: bool,
    pub played_development_this_turn: bool,
    /// Publicly inferred opponent-policy mixture:
    /// balanced, expansion, city/dev, trade-flexible, trade-resistant.
    pub policy_profile: [u8; 5],
}

impl PlayerState {
    pub const fn new() -> Self {
        Self {
            resources: [0; 5],
            development: [0; 5],
            bought_development: [0; 5],
            public_victory_points: 0,
            played_knights: 0,
            roads_left: 15,
            settlements_left: 5,
            cities_left: 4,
            has_longest_road: false,
            has_largest_army: false,
            played_development_this_turn: false,
            policy_profile: [51; 5],
        }
    }

    pub fn resource_total(&self) -> u8 {
        self.resources.iter().copied().sum()
    }

    pub fn hidden_victory_points(&self) -> u8 {
        self.development[DevCard::VictoryPoint.index()]
    }

    pub fn victory_points(&self) -> u8 {
        self.public_victory_points + self.hidden_victory_points()
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for PlayerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlayerState")
            .field("resources", &self.resources)
            .field("development", &self.development)
            .field("public_victory_points", &self.public_victory_points)
            .field("played_knights", &self.played_knights)
            .field(
                "pieces",
                &(self.roads_left, self.settlements_left, self.cities_left),
            )
            .finish()
    }
}

pub const ROAD_COST: ResourceHand = [1, 1, 0, 0, 0];
pub const SETTLEMENT_COST: ResourceHand = [1, 1, 1, 1, 0];
pub const CITY_COST: ResourceHand = [0, 0, 0, 2, 3];
pub const DEVELOPMENT_COST: ResourceHand = [0, 0, 1, 1, 1];
