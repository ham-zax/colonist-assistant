import process from "node:process";
import { writeFile } from "node:fs/promises";
import WebSocket from "ws";

const port = process.env.CDP_PORT ?? "9337";
const clickIndex = process.argv.indexOf("--click");
const screenshotIndex = process.argv.indexOf("--screenshot");
const screenshotPath =
  screenshotIndex >= 0 ? process.argv[screenshotIndex + 1] : undefined;
const click =
  clickIndex >= 0
    ? {
        x: Number(process.argv[clickIndex + 1]),
        y: Number(process.argv[clickIndex + 2]),
      }
    : undefined;
const expression = click
  ? process.argv.slice(clickIndex + 3).join(" ") || "true"
  : screenshotPath
    ? "true"
  : process.argv.slice(2).join(" ");
if (
  !expression ||
  (click && (!Number.isFinite(click.x) || !Number.isFinite(click.y))) ||
  (screenshotIndex >= 0 && !screenshotPath)
) {
  throw new Error(
    "Pass a JavaScript expression, --click X Y, or --screenshot PATH.",
  );
}

const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then(
  (response) => response.json(),
);
const target = targets.find(
  (candidate) =>
    candidate.type === "page" &&
    candidate.url.startsWith("https://colonist.io/") &&
    !candidate.parentId,
);
if (!target?.webSocketDebuggerUrl) {
  throw new Error("No top-level Colonist page is attached.");
}

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.once("open", resolve);
  socket.once("error", reject);
});

let nextId = 1;
const send = (method, params) => {
  const id = nextId++;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => {
    const handler = (payload) => {
      const message = JSON.parse(payload.toString());
      if (message.id !== id) return;
      socket.off("message", handler);
      if (message.error) reject(new Error(message.error.message));
      else resolve(message);
    };
    socket.on("message", handler);
  });
};

if (click) {
  await send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: click.x,
    y: click.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: click.x,
    y: click.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}
if (screenshotPath) {
  const screenshot = await send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(
    screenshotPath,
    Buffer.from(screenshot.result.data, "base64"),
  );
}
const response = await send("Runtime.evaluate", {
  expression,
  awaitPromise: true,
  returnByValue: true,
  userGesture: true,
});
socket.close();

if (response.result?.exceptionDetails) {
  throw new Error(
    response.result.exceptionDetails.exception?.description ??
      response.result.exceptionDetails.text,
  );
}
console.log(JSON.stringify(response.result?.result?.value, null, 2));
