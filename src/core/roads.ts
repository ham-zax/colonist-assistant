import type { BoardEdge, BoardSnapshot } from "./placement";

/**
 * Exact longest-trail search for the base-game road graph. An opponent building
 * may terminate a route at its vertex, but never lets the route continue
 * through it. Edges, rather than vertices, are the no-repeat constraint.
 */
export const longestRoadFromEdges = (
  board: Pick<BoardSnapshot, "edges" | "vertices">,
  player: string,
): number => {
  const owned = board.edges.filter((edge) => edge.player === player);
  if (!owned.length) return 0;
  const byVertex = new Map<string, BoardEdge[]>();
  for (const edge of owned) {
    for (const vertex of edge.vertices) {
      byVertex.set(vertex, [...(byVertex.get(vertex) ?? []), edge]);
    }
  }
  const blocked = new Set(
    board.vertices
      .filter(
        (vertex) =>
          vertex.building && vertex.building.player !== player,
      )
      .map((vertex) => vertex.id),
  );
  const walk = (vertex: string, used: Set<string>): number => {
    if (used.size && blocked.has(vertex)) return used.size;
    let best = used.size;
    for (const edge of byVertex.get(vertex) ?? []) {
      if (used.has(edge.id)) continue;
      const next =
        edge.vertices[0] === vertex ? edge.vertices[1] : edge.vertices[0];
      const nextUsed = new Set(used);
      nextUsed.add(edge.id);
      best = Math.max(best, walk(next, nextUsed));
    }
    return best;
  };
  return Math.max(
    ...owned.flatMap((edge) =>
      edge.vertices.map((vertex) => walk(vertex, new Set())),
    ),
  );
};
