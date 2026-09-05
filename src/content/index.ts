import {
  installPublicBoardBridge,
  readPublicBoardSnapshot,
} from "./board";
import { findLogRoot } from "./dom";
import { AssistantOverlay } from "./overlay";
import { GameSession } from "./session";
import {
  readSettings,
  RESET_NONCE_KEY,
  SETTINGS_KEY,
  type AssistantSettings,
} from "./settings";
import { clearCurrentGameStorage } from "../core/local-data";
import { investigationRecorder } from "./investigation-recorder";

const boot = async (): Promise<void> => {
  if (document.getElementById("colonist-assistant-root")) return;
  let settings = await readSettings();
  let session: GameSession | undefined;
  let currentRoot: HTMLElement | undefined;
  let currentGameKey: string | undefined;
  let currentMyPlayer: string | undefined;
  let currentBoard = readPublicBoardSnapshot();
  const boardOnlyRoot = document.createElement("div");
  boardOnlyRoot.dataset.colonistAssistantBoardOnly = "true";
  let currentInitialPlacement = Boolean(currentBoard?.initialPlacement);
  currentGameKey = currentBoard?.gameKey;
  currentMyPlayer =
    currentBoard?.localSeatDiagnostics?.identity.status === "resolved"
      ? currentBoard.myPlayer
      : undefined;
  await investigationRecorder.initialize(
    settings.investigationLog,
    currentGameKey,
  );

  let overlay: AssistantOverlay;
  const clearCurrentSession = async (): Promise<void> => {
    await Promise.all([
      session
        ? session.clearStoredData()
        : clearCurrentGameStorage(),
      overlay.clearStoredSessionData(),
    ]);
  };
  overlay = new AssistantOverlay(settings, {
    reset: clearCurrentSession,
  });
  overlay.setSettings(settings);
  overlay.updateBoard(currentBoard);
  let boardPublicationActive = false;
  let pendingSessionUpdate: GameSession | undefined;
  const publishSessionUpdate = (updated: GameSession): void => {
    if (boardPublicationActive) {
      pendingSessionUpdate = updated;
      return;
    }
    overlay.update(updated);
  };
  const removeBoardBridge = installPublicBoardBridge((snapshot) => {
    boardPublicationActive = true;
    try {
      currentBoard = snapshot ?? readPublicBoardSnapshot();
      if (snapshot?.gameKey) {
        currentGameKey = snapshot.gameKey;
        investigationRecorder.setGame(snapshot.gameKey);
        session?.setGameKey(snapshot.gameKey);
      }
      const resolvedMyPlayer =
        snapshot?.localSeatDiagnostics?.identity.status === "resolved"
          ? snapshot.myPlayer
          : undefined;
      currentMyPlayer = resolvedMyPlayer;
      session?.setMyPlayer(resolvedMyPlayer);
      currentInitialPlacement = Boolean(currentBoard?.initialPlacement);
      session?.setInitialPlacement(currentInitialPlacement, currentBoard?.gameKey);
      if (
        (currentRoot === boardOnlyRoot || currentBoard?.botOnlyGame) &&
        currentBoard?.gameplayRollCount !== undefined
      ) {
        // Bot games can expose the authoritative board roll several seconds before
        // the virtualized game-log row hydrates. Capture that public roll now and
        // reconcile the later DOM presentation by gameplay ordinal.
        session?.observeBoardDiceSnapshot(currentBoard);
      }
      overlay.updateBoard(currentBoard);
    } finally {
      boardPublicationActive = false;
      const pending = pendingSessionUpdate;
      pendingSessionUpdate = undefined;
      if (pending) overlay.update(pending);
    }
  });

  const attach = async (): Promise<void> => {
    const hasLiveGameSurface = Boolean(
      document.querySelector(
        "#game-canvas, script[type='application/json'][data-colonist-public-board], [data-hex-id]",
      ),
    );
    const logRoot = hasLiveGameSurface ? findLogRoot() : undefined;
    // The chat/game-log virtualizer is optional and can attach late in any game.
    // Keep a real session alive from validated public board evidence so setup
    // authority, recording, and Balanced-Dice history do not depend on that DOM.
    const root = logRoot ?? (hasLiveGameSurface ? boardOnlyRoot : undefined);
    if (!settings.enabled) {
      session?.stop();
      session = undefined;
      currentRoot = undefined;
      overlay.update(undefined);
      overlay.updateBoard(undefined);
      return;
    }
    if (!hasLiveGameSurface) {
      session?.stop();
      session = undefined;
      currentRoot = undefined;
      currentGameKey = undefined;
      currentMyPlayer = undefined;
      overlay.update(undefined);
      overlay.updateBoard(undefined);
      return;
    }
    if (root === currentRoot) return;
    session?.stop();
    session = undefined;
    currentRoot = root;
    overlay.update(undefined);
    if (!root) return;
    const next = new GameSession(
      root,
      publishSessionUpdate,
      currentGameKey,
    );
    session = next;
    next.setInitialPlacement(currentInitialPlacement, currentGameKey);
    await next.start();
    next.setMyPlayer(currentMyPlayer);
    if (
      (root === boardOnlyRoot || currentBoard?.botOnlyGame) &&
      currentBoard?.gameplayRollCount !== undefined
    ) {
      next.observeBoardDiceSnapshot(currentBoard);
    }
  };

  await attach();
  const poll = window.setInterval(() => void attach(), 900);

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "sync" && changes[SETTINGS_KEY]?.newValue) {
      settings = {
        ...settings,
        ...(changes[SETTINGS_KEY].newValue as Partial<AssistantSettings>),
      };
      investigationRecorder.setEnabled(settings.investigationLog);
      overlay.setSettings(settings);
      void attach();
    }
    if (area === "sync" && changes[RESET_NONCE_KEY]) {
      void clearCurrentSession();
    }
  });

  window.addEventListener(
    "pagehide",
    () => {
      window.clearInterval(poll);
      removeBoardBridge();
      session?.stop();
      if (investigationRecorder.isEnabled()) void investigationRecorder.flush();
      overlay.destroy();
    },
    { once: true },
  );
};

void boot();
