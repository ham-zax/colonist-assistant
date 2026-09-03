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

const boot = async (): Promise<void> => {
  if (document.getElementById("colonist-assistant-root")) return;
  let settings = await readSettings();
  let session: GameSession | undefined;
  let currentRoot: HTMLElement | undefined;
  let currentGameKey: string | undefined;
  let currentMyPlayer: string | undefined;
  const initialBoard = readPublicBoardSnapshot();
  currentGameKey = initialBoard?.gameKey;
  currentMyPlayer =
    initialBoard?.localSeatDiagnostics?.identity.status === "resolved"
      ? initialBoard.myPlayer
      : undefined;

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
  overlay.updateBoard(initialBoard);
  const removeBoardBridge = installPublicBoardBridge((snapshot) => {
    if (snapshot?.gameKey) {
      currentGameKey = snapshot.gameKey;
      session?.setGameKey(snapshot.gameKey);
    }
    const resolvedMyPlayer =
      snapshot?.localSeatDiagnostics?.identity.status === "resolved"
        ? snapshot.myPlayer
        : undefined;
    currentMyPlayer = resolvedMyPlayer;
    session?.setMyPlayer(resolvedMyPlayer);
    overlay.updateBoard(snapshot ?? readPublicBoardSnapshot());
  });

  const attach = async (): Promise<void> => {
    const hasLiveGameSurface = Boolean(
      document.querySelector(
        "#game-canvas, script[type='application/json'][data-colonist-public-board], [data-hex-id]",
      ),
    );
    const root = hasLiveGameSurface ? findLogRoot() : undefined;
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
      (updated) => overlay.update(updated),
      currentGameKey,
    );
    session = next;
    await next.start();
    next.setMyPlayer(currentMyPlayer);
  };

  await attach();
  const poll = window.setInterval(() => void attach(), 900);

  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "sync" && changes[SETTINGS_KEY]?.newValue) {
      settings = {
        ...settings,
        ...(changes[SETTINGS_KEY].newValue as Partial<AssistantSettings>),
      };
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
      overlay.destroy();
    },
    { once: true },
  );
};

void boot();
