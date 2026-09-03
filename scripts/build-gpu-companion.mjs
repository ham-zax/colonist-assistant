import { access } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const engine = join(root, "engine");
const cargoHome = process.env.CARGO_HOME ?? join(process.env.HOME ?? "", ".cargo");
const output = join(engine, "target", "release", "colonist-assistant-gpu");

const executable = async (preferred, fallback) => {
  try {
    await access(preferred);
    return preferred;
  } catch {
    return fallback;
  }
};

const run = (command, arguments_, cwd) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { cwd, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with code ${code}`));
    });
  });

const cargo = await executable(join(cargoHome, "bin", "cargo"), "cargo");

await run(
  cargo,
  ["build", "-p", "colonist-catan-native-host", "--release"],
  engine,
);
await access(output);

console.log(`Built Windows Runtime WSL GPU companion: ${output}`);
