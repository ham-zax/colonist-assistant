// Shared request/response contract for both builds. Runtime exports are generated
// by wasm-bindgen; thread initialization is the threaded build's extra entrypoint.
export * from "../wasm/colonist_search.js";
export { default } from "../wasm/colonist_search.js";
export function initThreadPool(threadCount: number): Promise<void>;
