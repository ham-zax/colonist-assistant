export const EXTENSION_CONTEXT_RELOAD_MESSAGE =
  "The extension was reloaded or updated. Reload this Colonist tab once to reconnect the content script to the background WASM engine.";

export const isExtensionContextInvalidatedError = (
  error: unknown,
): boolean => {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "";
  return /extension context invalidated/iu.test(message);
};
