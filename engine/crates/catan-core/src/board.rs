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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub num_players: u8,
    pub hexes: Vec<Hex>,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

impl Board {
    pub fn standard(seed: u64, num_players: u8) -> Self {
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
        let mut numbers = vec![2, 3, 3, 4, 4, 5, 5, 6, 6, 8, 8, 9, 9, 10, 10, 11, 11, 12];
        rng.shuffle(&mut numbers);

        let mut axial = Vec::new();
        for q in -2i8..=2 {
            let r_min = (-2i8).max(-q - 2);
            let r_max = 2i8.min(-q + 2);
            for r in r_min..=r_max {
                axial.push((q, r));
            }
        }
        axial.sort_by_key(|(q, r)| (*r, *q));

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

#[cfg(test)]
mod tests {
    use crate::Board;

    #[test]
    fn standard_board_has_canonical_topology() {
        let board = Board::standard(1, 4);
        assert_eq!(board.hexes.len(), 19);
        assert_eq!(board.vertices.len(), 54);
        assert_eq!(board.edges.len(), 72);
        assert_eq!(
            board
                .vertices
                .iter()
                .filter(|vertex| vertex.port.is_some())
                .count(),
            18
        );
        assert_eq!(
            board
                .hexes
                .iter()
                .filter(|hex| hex.resource.is_none())
                .count(),
            1
        );
    }

    #[test]
    fn standard_board_seed_is_reproducible() {
        assert_eq!(Board::standard(77, 3), Board::standard(77, 3));
        assert_ne!(Board::standard(77, 3), Board::standard(78, 3));
    }
}
