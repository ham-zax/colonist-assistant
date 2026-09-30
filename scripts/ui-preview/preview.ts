import { installChromeStub } from "./chrome-stub";
import {
  isScenarioName,
  mountScenario,
  SCENARIO_NAMES,
  type ScenarioName,
} from "./scenarios";
import type { BoardSnapshot } from "../../src/core/placement";
import { PLAYER_COLORS, PLAYERS } from "./fixtures";

const RESOURCE_FILL: Record<string, string> = {
  lumber: "#3f7d3a",
  brick: "#b5562e",
  wool: "#9ccc65",
  grain: "#e3c34f",
  ore: "#8e9aa6",
};

const SVG_NS = "http://www.w3.org/2000/svg";

const svgElement = (
  tag: string,
  attributes: Record<string, string | number>,
  text?: string,
): SVGElement => {
  const element = document.createElementNS(SVG_NS, tag);
  for (const [key, value] of Object.entries(attributes)) {
    element.setAttribute(key, String(value));
  }
  if (text !== undefined) element.textContent = text;
  return element;
};

/** Plain-SVG stand-in for Colonist's board so board markers have context. */
const drawBoard = (board: BoardSnapshot): SVGElement => {
  const svg = svgElement("svg", {
    width: window.innerWidth,
    height: window.innerHeight,
    style: "position:fixed;inset:0;pointer-events:none",
  });
  const vertexById = new Map(board.vertices.map((vertex) => [vertex.id, vertex]));
  for (const hex of board.hexes) {
    if (!hex.screen) continue;
    const points = Array.from({ length: 6 }, (_, i) => {
      const angle = ((60 * i - 30) * Math.PI) / 180;
      return `${hex.screen!.x + 55 * Math.cos(angle)},${hex.screen!.y + 55 * Math.sin(angle)}`;
    }).join(" ");
    svg.append(
      svgElement("polygon", {
        points,
        fill: hex.resource ? RESOURCE_FILL[hex.resource]! : "#e6d3a1",
        stroke: "#1d4f7a",
        "stroke-width": 2,
      }),
    );
    if (hex.number) {
      svg.append(
        svgElement("circle", { cx: hex.screen.x, cy: hex.screen.y, r: 15, fill: "#f7efd8" }),
        svgElement(
          "text",
          {
            x: hex.screen.x,
            y: hex.screen.y + 5,
            "text-anchor": "middle",
            "font-size": 15,
            "font-weight": 700,
            fill: hex.number === 6 || hex.number === 8 ? "#c62828" : "#1d1d1d",
            "font-family": "sans-serif",
          },
          String(hex.number),
        ),
      );
    }
  }
  for (const edge of board.edges) {
    if (!edge.player) continue;
    const [a, b] = edge.vertices.map((id) => vertexById.get(id)?.screen);
    if (!a || !b) continue;
    svg.append(
      svgElement("line", {
        x1: a.x, y1: a.y, x2: b.x, y2: b.y,
        stroke: PLAYER_COLORS[edge.player] ?? "#fff",
        "stroke-width": 7,
        "stroke-linecap": "round",
      }),
    );
  }
  for (const vertex of board.vertices) {
    if (!vertex.building || !vertex.screen) continue;
    svg.append(
      svgElement("circle", {
        cx: vertex.screen.x,
        cy: vertex.screen.y,
        r: vertex.building.kind === "city" ? 12 : 8,
        fill: PLAYER_COLORS[vertex.building.player] ?? "#fff",
        stroke: "#10202c",
        "stroke-width": 2,
      }),
    );
  }
  return svg;
};

/** Left-hand player list; win-odds badges attach to panels found by player name. */
const drawPlayerPanels = (board: BoardSnapshot): HTMLElement => {
  const column = document.createElement("div");
  column.style.cssText =
    "position:fixed;left:16px;top:72px;display:flex;flex-direction:column;gap:8px;font-family:system-ui,sans-serif";
  for (const player of PLAYERS) {
    const panel = document.createElement("div");
    panel.style.cssText = `width:240px;height:64px;box-sizing:border-box;padding:8px 12px;border-radius:8px;background:rgba(16,32,44,.88);border-left:6px solid ${PLAYER_COLORS[player]};color:#fff;font-size:14px;overflow:hidden`;
    const points = board.players?.[player]?.visiblePoints ?? 0;
    panel.innerHTML = `<div style="font-weight:700;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"></div><div style="opacity:.7;font-size:12px;margin-top:4px">${points} VP</div>`;
    panel.firstElementChild!.textContent = player;
    column.append(panel);
  }
  return column;
};

const params = new URLSearchParams(location.search);
const requested = params.get("state");
const state: ScenarioName = isScenarioName(requested) ? requested : "build";

document.title = `Colonist Ally preview: ${state}`;
document.body.style.cssText =
  "margin:0;width:100vw;height:100vh;overflow:hidden;background:#2f7fc1";
installChromeStub((path) => path);

const { board } = mountScenario(state);
if (board) {
  document.body.append(drawBoard(board), drawPlayerPanels(board));
}
(window as unknown as { __previewScenarios: readonly string[] }).__previewScenarios =
  SCENARIO_NAMES;
