import { access } from "node:fs/promises";
import { spawn } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cargoHome = process.env.CARGO_HOME ?? join(process.env.HOME ?? "", ".cargo");
const preferredCargo = join(cargoHome, "bin", "cargo");

const cargo = await access(preferredCargo).then(() => preferredCargo).catch(() => "cargo");

await new Promise((resolve, reject) => {
  const child = spawn(cargo, ["build", "--release", "-p", "colonist-catan-arena"], {
    cwd: join(root, "engine"),
    stdio: "inherit",
  });
  child.once("error", reject);
  child.once("exit", (code) => {
    if (code === 0) resolve();
    else reject(new Error(`${cargo} exited with code ${code}`));
  });
});

console.log("Built native release arena binary");
