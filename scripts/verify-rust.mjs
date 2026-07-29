import { access } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const engine = join(root, "engine");
const cargoHome = process.env.CARGO_HOME ?? join(process.env.HOME ?? "", ".cargo");
const preferredCargo = join(cargoHome, "bin", "cargo");
let cargo = "cargo";
try {
  await access(preferredCargo);
  cargo = preferredCargo;
} catch {
  // Fall back to PATH on standard Rust installations.
}

const run = (arguments_) =>
  new Promise((resolve, reject) => {
    const child = spawn(cargo, arguments_, {
      cwd: engine,
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${cargo} exited with code ${code}`));
    });
  });

await run(["fmt", "--all", "--", "--check"]);
await run(["test", "--workspace"]);
await run(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
