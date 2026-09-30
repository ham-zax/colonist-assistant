const FONT_FAMILY = "Archivo Narrow";
const FONT_PATH = "assets/fonts/ArchivoNarrow-Variable.ttf";

let registration: Promise<void> | undefined;

/**
 * Chrome ignores `@font-face` rules declared inside a shadow root, so the
 * overlay's bundled font must be registered on the document. The bytes are
 * read from the extension package, so the page's font CSP never applies.
 */
export const ensureOverlayFont = (): Promise<void> => {
  registration ??= (async () => {
    if (typeof FontFace === "undefined" || !document.fonts) return;
    const loaded = [...document.fonts].some(
      (face) => face.family.replaceAll('"', "") === FONT_FAMILY,
    );
    if (loaded) return;
    const response = await fetch(chrome.runtime.getURL(FONT_PATH));
    if (!response.ok) return;
    const face = new FontFace(FONT_FAMILY, await response.arrayBuffer(), {
      weight: "400 700",
      display: "swap",
    });
    document.fonts.add(await face.load());
  })().catch(() => {
    // The overlay stays usable with the fallback font stack.
  });
  return registration;
};
