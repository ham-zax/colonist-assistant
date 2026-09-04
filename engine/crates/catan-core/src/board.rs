use std::collections::{BTreeMap, BTreeSet};

use crate::{Port, Resource, SplitMix64};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hex {
    pub resource: Option<Resource>,
    pub number: u8,
    pub coord: (i8, i8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vertex {
    pub adjacent_hexes: Vec<u8>,
    pub adjacent_vertices: Vec<u8>,
    pub adjacent_edges: Vec<u8>,
    pub port: Option<Port>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub vertices: [u8; 2],
    pub adjacent_hexes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyntheticBoardGenerator {
    #[default]
    LegacyRandomizedV1,
    Classic4pV1,
}

impl SyntheticBoardGenerator {
    pub const LEGACY_RANDOMIZED_V1_ID: &'static str = "legacy-randomized-v1";
    pub const CLASSIC_4P_V1_ID: &'static str = "classic4p-v1";

    pub const fn serialized_id(self) -> &'static str {
        match self {
            Self::LegacyRandomizedV1 => Self::LEGACY_RANDOMIZED_V1_ID,
            Self::Classic4pV1 => Self::CLASSIC_4P_V1_ID,
        }
    }

    pub fn parse_serialized_id(value: &str) -> Result<Self, String> {
        match value {
            Self::LEGACY_RANDOMIZED_V1_ID => Ok(Self::LegacyRandomizedV1),
            Self::CLASSIC_4P_V1_ID => Ok(Self::Classic4pV1),
            _ => Err(format!("unsupported synthetic board generator: {value}")),
        }
    }

    /// Binds a state hash to this generator contract for persisted provenance.
    ///
    /// This is an identity checksum, not a cryptographic signature. It prevents
    /// a snapshot from silently changing generator metadata when two generators
    /// happen to produce the same structural board for a seed.
    pub fn provenance_state_hash(self, state_hash: u64) -> u64 {
        let mut hash = state_hash ^ 0xcbf2_9ce4_8422_2325;
        for byte in self.serialized_id().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub num_players: u8,
    pub hexes: Vec<Hex>,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

impl Board {
    /// Compatibility alias for the historical seeded synthetic generator.
    ///
    /// New semantic callers should name `randomized_base_v1` or `classic_4p_v1`
    /// explicitly. This alias remains legacy-randomized-v1 so existing fixtures,
    /// persisted seeds, and stress corpora are not reinterpreted.
    pub fn standard(seed: u64, num_players: u8) -> Self {
        Self::randomized_base_v1(seed, num_players)
    }

    pub fn randomized_base_v1(seed: u64, num_players: u8) -> Self {
        Self::generate_base_v1(seed, num_players, false)
    }

    /// Repository-defined legal Classic4P synthetic board generator.
    ///
    /// This contract guarantees the ordinary base-game multisets and separates
    /// every red 6/8 token from every other red token. It does not claim to
    /// reproduce Colonist's private/server-side seed algorithm.
    pub fn classic_4p_v1(seed: u64) -> Self {
        Self::generate_base_v1(seed, 4, true)
    }

    pub fn from_generator(
        generator: SyntheticBoardGenerator,
        seed: u64,
        num_players: u8,
    ) -> Result<Self, String> {
        match generator {
            SyntheticBoardGenerator::LegacyRandomizedV1 => {
                if !(2..=4).contains(&num_players) {
                    return Err(format!(
                        "legacy-randomized-v1 supports 2-4 players, got {num_players}"
                    ));
                }
                Ok(Self::randomized_base_v1(seed, num_players))
            }
            SyntheticBoardGenerator::Classic4pV1 => {
                if num_players != 4 {
                    return Err(format!(
                        "classic4p-v1 requires 4 players, got {num_players}"
                    ));
                }
                Ok(Self::classic_4p_v1(seed))
            }
        }
    }

    fn generate_base_v1(seed: u64, num_players: u8, separate_red_numbers: bool) -> Self {
        assert!((2..=4).contains(&num_players));
        let mut rng = SplitMix64::new(seed);
        let mut resources = vec![
            Resource::Lumber,
            Resource::Lumber,
            Resource::Lumber,
            Resource::Lumber,
            Resource::Brick,
            Resource::Brick,
            Resource::Brick,
            Resource::Wool,
            Resource::Wool,
            Resource::Wool,
            Resource::Wool,
            Resource::Grain,
            Resource::Grain,
            Resource::Grain,
            Resource::Grain,
            Resource::Ore,
            Resource::Ore,
            Resource::Ore,
        ];
        rng.shuffle(&mut resources);
        let desert_index = rng.range(19);

        let axial = base_axial_coords();
        let numbers = if separate_red_numbers {
            loop {
                let mut candidate = number_tokens();
                rng.shuffle(&mut candidate);
                if !has_adjacent_red_numbers(&axial, desert_index, &candidate) {
                    break candidate;
                }
            }
        } else {
            let mut candidate = number_tokens();
            rng.shuffle(&mut candidate);
            candidate
        };

        let mut resource_cursor = 0;
        let mut number_cursor = 0;
        let hexes = axial
            .iter()
            .enumerate()
            .map(|(index, coord)| {
                if index == desert_index {
                    Hex {
                        resource: None,
                        number: 0,
                        coord: *coord,
                    }
                } else {
                    let hex = Hex {
                        resource: Some(resources[resource_cursor]),
                        number: numbers[number_cursor],
                        coord: *coord,
                    };
                    resource_cursor += 1;
                    number_cursor += 1;
                    hex
                }
            })
            .collect::<Vec<_>>();

        let corner_offsets = [(1, 1), (-1, 1), (-2, 0), (-1, -1), (1, -1), (2, 0)];
        let mut vertex_ids = BTreeMap::<(i16, i16), u8>::new();
        let mut vertex_hexes = Vec::<BTreeSet<u8>>::new();
        let mut edge_ids = BTreeMap::<(u8, u8), u8>::new();
        let mut edge_hexes = Vec::<BTreeSet<u8>>::new();

        for (hex_index, (q, r)) in axial.iter().copied().enumerate() {
            let center = (3 * q as i16, 2 * r as i16 + q as i16);
            let corners = corner_offsets.map(|(x, y)| {
                let coord = (center.0 + x, center.1 + y);
                if let Some(index) = vertex_ids.get(&coord) {
                    *index
                } else {
                    let index = vertex_ids.len() as u8;
                    vertex_ids.insert(coord, index);
                    vertex_hexes.push(BTreeSet::new());
                    index
                }
            });
            for corner in corners {
                vertex_hexes[corner as usize].insert(hex_index as u8);
            }
            for index in 0..6 {
                let a = corners[index];
                let b = corners[(index + 1) % 6];
                let key = if a < b { (a, b) } else { (b, a) };
                let edge = if let Some(edge) = edge_ids.get(&key) {
                    *edge
                } else {
                    let edge = edge_ids.len() as u8;
                    edge_ids.insert(key, edge);
                    edge_hexes.push(BTreeSet::new());
                    edge
                };
                edge_hexes[edge as usize].insert(hex_index as u8);
            }
        }

        let edges = edge_ids
            .iter()
            .map(|(&(a, b), &index)| {
                (
                    index,
                    Edge {
                        vertices: [a, b],
                        adjacent_hexes: edge_hexes[index as usize].iter().copied().collect(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let edges = (0..edges.len() as u8)
            .map(|index| edges[&index].clone())
            .collect::<Vec<_>>();

        let mut vertices = (0..vertex_ids.len())
            .map(|index| Vertex {
                adjacent_hexes: vertex_hexes[index].iter().copied().collect(),
                adjacent_vertices: Vec::new(),
                adjacent_edges: Vec::new(),
                port: None,
            })
            .collect::<Vec<_>>();
        for (edge_index, edge) in edges.iter().enumerate() {
            let [a, b] = edge.vertices;
            vertices[a as usize].adjacent_vertices.push(b);
            vertices[b as usize].adjacent_vertices.push(a);
            vertices[a as usize].adjacent_edges.push(edge_index as u8);
            vertices[b as usize].adjacent_edges.push(edge_index as u8);
        }
        for vertex in &mut vertices {
            vertex.adjacent_vertices.sort_unstable();
            vertex.adjacent_edges.sort_unstable();
        }

        let mut coast = edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.adjacent_hexes.len() == 1)
            .map(|(index, edge)| (index as u8, edge.vertices))
            .collect::<Vec<_>>();
        let first = coast
            .iter()
            .min_by_key(|(_, [a, b])| (*a).min(*b))
            .copied()
            .expect("standard board has a coastline");
        let mut ordered = vec![first];
        coast.retain(|item| item.0 != first.0);
        while let Some((_, previous)) = ordered.last().copied() {
            let tail = previous[1];
            let found = coast
                .iter()
                .position(|(_, [a, b])| *a == tail || *b == tail);
            let Some(index) = found else {
                break;
            };
            let (edge, [a, b]) = coast.remove(index);
            ordered.push((edge, if a == tail { [a, b] } else { [b, a] }));
        }
        if ordered.len() != 30 {
            ordered = edges
                .iter()
                .enumerate()
                .filter(|(_, edge)| edge.adjacent_hexes.len() == 1)
                .map(|(index, edge)| (index as u8, edge.vertices))
                .collect();
        }
        let mut ports = vec![
            Port::Generic,
            Port::Generic,
            Port::Generic,
            Port::Generic,
            Port::Resource(Resource::Lumber),
            Port::Resource(Resource::Brick),
            Port::Resource(Resource::Wool),
            Port::Resource(Resource::Grain),
            Port::Resource(Resource::Ore),
        ];
        rng.shuffle(&mut ports);
        for (port_index, coast_index) in [0usize, 3, 7, 10, 13, 17, 20, 23, 27]
            .into_iter()
            .enumerate()
        {
            let edge = &edges[ordered[coast_index % ordered.len()].0 as usize];
            vertices[edge.vertices[0] as usize].port = Some(ports[port_index]);
            vertices[edge.vertices[1] as usize].port = Some(ports[port_index]);
        }

        Self {
            num_players,
            hexes,
            vertices,
            edges,
        }
    }
}

fn base_axial_coords() -> Vec<(i8, i8)> {
    let mut axial = Vec::new();
    for q in -2i8..=2 {
        let r_min = (-2i8).max(-q - 2);
        let r_max = 2i8.min(-q + 2);
        for r in r_min..=r_max {
            axial.push((q, r));
        }
    }
    axial.sort_by_key(|(q, r)| (*r, *q));
    axial
}

fn number_tokens() -> Vec<u8> {
    vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 8, 8, 9, 9, 10, 10, 11, 11, 12]
}

fn axial_neighbors(left: (i8, i8), right: (i8, i8)) -> bool {
    let dq = (left.0 - right.0).abs();
    let dr = (left.1 - right.1).abs();
    let ds = (left.0 + left.1 - right.0 - right.1).abs();
    dq.max(dr).max(ds) == 1
}

fn has_adjacent_red_numbers(axial: &[(i8, i8)], desert_index: usize, numbers: &[u8]) -> bool {
    let mut red_coords = Vec::with_capacity(4);
    let mut number_cursor = 0usize;
    for (hex_index, coord) in axial.iter().copied().enumerate() {
        if hex_index == desert_index {
            continue;
        }
        let number = numbers[number_cursor];
        number_cursor += 1;
        if matches!(number, 6 | 8) {
            red_coords.push(coord);
        }
    }
    red_coords.iter().enumerate().any(|(index, left)| {
        red_coords[index + 1..]
            .iter()
            .any(|right| axial_neighbors(*left, *right))
    })
}

#[cfg(test)]
mod tests {
    use crate::{Board, Port, Resource, SyntheticBoardGenerator};

    use super::axial_neighbors;

    fn assert_classic_4p_v1_legal(board: &Board) {
        assert_eq!(board.num_players, 4);
        assert_eq!(board.hexes.len(), 19);
        assert_eq!(board.vertices.len(), 54);
        assert_eq!(board.edges.len(), 72);

        let mut resources = [0u8; 5];
        let mut desert_count = 0u8;
        let mut numbers = [0u8; 13];
        let mut red_coords = Vec::new();
        for hex in &board.hexes {
            match hex.resource {
                Some(resource) => {
                    resources[resource.index()] += 1;
                    assert!((2..=12).contains(&hex.number) && hex.number != 7);
                    numbers[hex.number as usize] += 1;
                    if matches!(hex.number, 6 | 8) {
                        red_coords.push(hex.coord);
                    }
                }
                None => {
                    desert_count += 1;
                    assert_eq!(hex.number, 0);
                }
            }
        }
        assert_eq!(resources, [4, 3, 4, 4, 3]);
        assert_eq!(desert_count, 1);
        assert_eq!(numbers, [0, 0, 1, 2, 2, 2, 2, 0, 2, 2, 2, 2, 1]);
        assert_eq!(red_coords.len(), 4);
        for (index, left) in red_coords.iter().enumerate() {
            for right in &red_coords[index + 1..] {
                assert!(!axial_neighbors(*left, *right));
            }
        }

        let mut port_vertices = [0u8; 6];
        for vertex in &board.vertices {
            if let Some(port) = vertex.port {
                let index = match port {
                    Port::Generic => 0,
                    Port::Resource(resource) => resource.index() + 1,
                };
                port_vertices[index] += 1;
            }
        }
        assert_eq!(port_vertices, [8, 2, 2, 2, 2, 2]);
        let harbor_edges = board
            .edges
            .iter()
            .filter(|edge| {
                edge.adjacent_hexes.len() == 1
                    && board.vertices[edge.vertices[0] as usize].port.is_some()
                    && board.vertices[edge.vertices[0] as usize].port
                        == board.vertices[edge.vertices[1] as usize].port
            })
            .count();
        assert_eq!(harbor_edges, 9);
    }

    #[test]
    fn classic_4p_v1_is_legal_reproducible_and_varied() {
        let first = Board::classic_4p_v1(77);
        assert_classic_4p_v1_legal(&first);
        assert_eq!(first, Board::classic_4p_v1(77));
        assert_ne!(first, Board::classic_4p_v1(78));
        for seed in 0..256 {
            assert_classic_4p_v1_legal(&Board::classic_4p_v1(seed));
        }
    }

    #[test]
    fn legacy_randomized_v1_preserves_seed_108100001_board_identity() {
        let board = Board::randomized_base_v1(108_100_001, 3);
        let hexes = board
            .hexes
            .iter()
            .map(|hex| (hex.resource.map(|resource| resource as u8), hex.number))
            .collect::<Vec<_>>();
        assert_eq!(
            hexes,
            vec![
                (Some(1), 10),
                (Some(4), 3),
                (Some(2), 11),
                (Some(1), 11),
                (Some(2), 9),
                (Some(4), 8),
                (Some(0), 5),
                (Some(3), 12),
                (Some(3), 9),
                (Some(3), 4),
                (Some(3), 3),
                (Some(1), 6),
                (Some(2), 10),
                (Some(0), 4),
                (None, 0),
                (Some(4), 8),
                (Some(2), 6),
                (Some(0), 5),
                (Some(0), 2),
            ]
        );
        let ports = board
            .vertices
            .iter()
            .enumerate()
            .filter_map(|(index, vertex)| {
                vertex.port.map(|port| {
                    let code = match port {
                        Port::Generic => 1,
                        Port::Resource(resource) => resource as u8 + 2,
                    };
                    (index, code)
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ports,
            vec![
                (2, 1),
                (3, 1),
                (5, 4),
                (8, 4),
                (10, 1),
                (13, 1),
                (16, 1),
                (22, 3),
                (28, 1),
                (36, 2),
                (37, 3),
                (39, 6),
                (40, 6),
                (45, 2),
                (47, 1),
                (48, 1),
                (50, 5),
                (53, 5),
            ]
        );
        assert_eq!(board, Board::standard(108_100_001, 3));
    }

    #[test]
    fn generator_identifiers_and_player_contracts_are_explicit() {
        assert_eq!(
            SyntheticBoardGenerator::parse_serialized_id("legacy-randomized-v1"),
            Ok(SyntheticBoardGenerator::LegacyRandomizedV1)
        );
        assert_eq!(
            SyntheticBoardGenerator::parse_serialized_id("classic4p-v1"),
            Ok(SyntheticBoardGenerator::Classic4pV1)
        );
        assert!(SyntheticBoardGenerator::parse_serialized_id("classic4p-v2").is_err());
        assert_ne!(
            SyntheticBoardGenerator::LegacyRandomizedV1.provenance_state_hash(123),
            SyntheticBoardGenerator::Classic4pV1.provenance_state_hash(123)
        );
        assert!(Board::from_generator(SyntheticBoardGenerator::Classic4pV1, 1, 3).is_err());
        assert_eq!(
            Board::from_generator(SyntheticBoardGenerator::LegacyRandomizedV1, 77, 3).unwrap(),
            Board::randomized_base_v1(77, 3)
        );
    }

    #[test]
    fn resource_enum_order_remains_the_legacy_encoding() {
        assert_eq!(Resource::Lumber as u8, 0);
        assert_eq!(Resource::Brick as u8, 1);
        assert_eq!(Resource::Wool as u8, 2);
        assert_eq!(Resource::Grain as u8, 3);
        assert_eq!(Resource::Ore as u8, 4);
    }
}
