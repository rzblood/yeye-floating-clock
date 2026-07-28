import { cp, mkdir, readdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = join(root, "src");
const assets = join(root, "assets");
const output = join(root, "frontend-dist");

const runtimeAssets = [
  "pet-body-no-arms.png",
];

await rm(output, { recursive: true, force: true });
await mkdir(join(output, "assets"), { recursive: true });

for (const name of await readdir(source)) {
  if (name.endsWith(".html") || name.endsWith(".css") || name.endsWith(".js")) {
    await cp(join(source, name), join(output, name));
  }
}

for (const name of runtimeAssets) {
  await cp(join(assets, name), join(output, "assets", name));
}

await cp(join(assets, "weather"), join(output, "assets", "weather"), { recursive: true });
await mkdir(join(output, "assets", "puppet"), { recursive: true });
for (const name of ["left-upper.png", "left-joint.png", "right-joint.png"]) {
  await cp(join(assets, "puppet", name), join(output, "assets", "puppet", name));
}
