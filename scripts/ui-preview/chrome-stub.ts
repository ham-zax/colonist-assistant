/**
 * Minimal `chrome` global so the real content-script overlay can run in a
 * plain page (or jsdom). Nothing here talks to a network or a real extension:
 * every storage call resolves empty and the engine warm-up reports a ready
 * WASM runtime without loading any WASM.
 */
export const installChromeStub = (
  resolveUrl: (path: string) => string = (path) => path,
): void => {
  const storageArea = {
    get: () => Promise.resolve({}),
    set: () => Promise.resolve(),
    remove: () => Promise.resolve(),
  };
  const stub = {
    runtime: {
      getURL: resolveUrl,
      getManifest: () => ({
        manifest_version: 3,
        name: "Colonist Assistant",
        version: "preview",
      }),
      sendMessage: (message: { id?: number }) =>
        Promise.resolve({
          id: message.id,
          runtime: "background-wasm",
          engineRevision: "ui-preview",
          initializationMs: 1,
        }),
    },
    storage: {
      local: storageArea,
      sync: storageArea,
      onChanged: { addListener: () => undefined },
    },
  };
  (globalThis as unknown as { chrome: unknown }).chrome = stub;
};
