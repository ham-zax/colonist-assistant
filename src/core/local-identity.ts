export type LocalIdentityStatus = "resolved" | "unresolved";

export type LocalIdentitySource =
  | "controller+account-user-id+store-roster"
  | "replay-perspective"
  | "none";

export interface LocalIdentityPlayerSignal {
  color: number;
  name?: string;
  userId?: string | number;
}

export interface LocalIdentityResolution {
  status: LocalIdentityStatus;
  reason:
    | "cross-checked"
    | "replay-perspective"
    | "invalid-my-color"
    | "my-color-not-in-play-order"
    | "my-color-player-missing"
    | "my-color-name-unresolved"
    | "color-name-mapping-disagreement"
    | "manager-cached-fallback"
    | "current-user-id-unavailable"
    | "current-user-not-in-player-roster"
    | "current-user-matches-multiple-colors"
    | "controller-color-account-color-disagreement";
  source: LocalIdentitySource;
  currentUserIdAvailable: boolean;
  currentUserMatchColors: number[];
  myColor?: number;
  myPlayer?: string;
  currentUserColor?: number;
  currentUserPlayer?: string;
}

export interface ResolveLocalIdentityInput {
  myColor: unknown;
  mappedMyPlayer?: string;
  playOrder: number[];
  players: LocalIdentityPlayerSignal[];
  currentUserId?: string | number;
  managerResolutionSource?: "cached-module" | "module-scan" | "cached-fallback";
  isReplay?: boolean;
}

const sameUserId = (
  left: string | number | undefined,
  right: string | number | undefined,
): boolean =>
  left !== undefined && right !== undefined && String(left) === String(right);

export const resolveLocalIdentity = (
  input: ResolveLocalIdentityInput,
): LocalIdentityResolution => {
  const currentUserId = input.currentUserId;
  const currentUserPlayers =
    currentUserId === undefined
      ? []
      : input.players.filter((player) =>
          sameUserId(player.userId, currentUserId),
        );
  const evidence = {
    currentUserIdAvailable: currentUserId !== undefined,
    currentUserMatchColors: currentUserPlayers.map((player) => player.color),
  };

  if (input.managerResolutionSource === "cached-fallback") {
    return {
      status: "unresolved",
      reason: "manager-cached-fallback",
      source: "none",
      ...evidence,
      ...(typeof input.myColor === "number" && Number.isInteger(input.myColor)
        ? { myColor: input.myColor }
        : {}),
      ...(input.mappedMyPlayer ? { myPlayer: input.mappedMyPlayer } : {}),
    };
  }

  if (typeof input.myColor !== "number" || !Number.isInteger(input.myColor)) {
    return {
      status: "unresolved",
      reason: "invalid-my-color",
      source: "none",
      ...evidence,
    };
  }

  const myColor = input.myColor;
  if (input.playOrder.filter((color) => color === myColor).length !== 1) {
    return {
      status: "unresolved",
      reason: "my-color-not-in-play-order",
      source: "none",
      ...evidence,
      myColor,
    };
  }

  const myColorPlayer = input.players.find((player) => player.color === myColor);
  if (!myColorPlayer) {
    return {
      status: "unresolved",
      reason: "my-color-player-missing",
      source: "none",
      ...evidence,
      myColor,
    };
  }
  if (!myColorPlayer.name || !input.mappedMyPlayer) {
    return {
      status: "unresolved",
      reason: "my-color-name-unresolved",
      source: "none",
      ...evidence,
      myColor,
    };
  }
  if (myColorPlayer.name !== input.mappedMyPlayer) {
    return {
      status: "unresolved",
      reason: "color-name-mapping-disagreement",
      source: "none",
      ...evidence,
      myColor,
      myPlayer: input.mappedMyPlayer,
    };
  }

  if (input.isReplay) {
    return {
      status: "resolved",
      reason: "replay-perspective",
      source: "replay-perspective",
      ...evidence,
      myColor,
      myPlayer: input.mappedMyPlayer,
    };
  }

  if (currentUserId === undefined) {
    return {
      status: "unresolved",
      reason: "current-user-id-unavailable",
      source: "none",
      ...evidence,
      myColor,
      myPlayer: input.mappedMyPlayer,
    };
  }
  if (currentUserPlayers.length === 0) {
    return {
      status: "unresolved",
      reason: "current-user-not-in-player-roster",
      source: "none",
      ...evidence,
      myColor,
      myPlayer: input.mappedMyPlayer,
    };
  }
  if (currentUserPlayers.length !== 1) {
    return {
      status: "unresolved",
      reason: "current-user-matches-multiple-colors",
      source: "none",
      ...evidence,
      myColor,
      myPlayer: input.mappedMyPlayer,
    };
  }

  const currentUserPlayer = currentUserPlayers[0]!;
  if (currentUserPlayer.color !== myColor) {
    return {
      status: "unresolved",
      reason: "controller-color-account-color-disagreement",
      source: "none",
      ...evidence,
      myColor,
      myPlayer: input.mappedMyPlayer,
      currentUserColor: currentUserPlayer.color,
      ...(currentUserPlayer.name
        ? { currentUserPlayer: currentUserPlayer.name }
        : {}),
    };
  }

  return {
    status: "resolved",
    reason: "cross-checked",
    source: "controller+account-user-id+store-roster",
    ...evidence,
    myColor,
    myPlayer: input.mappedMyPlayer,
    currentUserColor: currentUserPlayer.color,
    ...(currentUserPlayer.name
      ? { currentUserPlayer: currentUserPlayer.name }
      : {}),
  };
};
