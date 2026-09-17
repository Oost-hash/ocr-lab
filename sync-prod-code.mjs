// Copy whole production files without rewriting content or line endings.
import { readFile, writeFile, mkdir, readdir } from "node:fs/promises";
import { resolve, dirname, relative, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

const root = dirname(fileURLToPath(import.meta.url));
const source = resolve(root, "../project");
const destination = resolve(root, "src-tauri/src/prod-code");
const manifestPath = resolve(root, "prod-code-manifest.json");
const files = [
  "ocr/mod.rs", "ocr/windows.rs", "ocr/linux.rs", "ocr_fallback.rs",
  "reward_watcher.rs", "log_watcher.rs", "monitor.rs",
  "platform/mod.rs", "platform/windows.rs", "platform/linux.rs",
  "diagnostics.rs", "relic_pick.rs", "worldstate.rs", "wfcd.rs", "catalogue.rs", "inventory_state.rs", "lib.rs",
  "app_state.rs", "resources/corrections.json",
  ...[
    "relic-overlay/Overlay.tsx", "relic-overlay/Overlay.css", "hooks/useOverlays.ts",
    "relic-overlay/RelicPickOverlay.tsx", "relic-overlay/RelicPickOverlay.css", "constants/settings.ts",
    "lib/uiScale.ts", "lib/rivenWindow.ts", "constants/preferences.ts", "constants/tauri.ts",
    "types/items.ts", "types/settings.ts", "types/tauri.ts", "types/inventory.ts", "types/relics.ts",
    "types/rivens.ts", "types/trades.ts", "types/market.ts", "types/filterPresets.ts", "types/filters.ts",
  ].map((file) => `frontend/${file}`),
  "frontend/public/platinum.webp", "frontend/public/ducats.webp",
];
const sourcePath = (file) => file.startsWith("frontend/public/")
  ? file.slice("frontend/".length)
  : file.startsWith("frontend/") ? `src/${file.slice("frontend/".length)}`
  : file.startsWith("resources/") ? `src-tauri/${file}` : `src-tauri/src/${file}`;
const check = process.argv.includes("--check");
const hash = (data) => createHash("sha256").update(data).digest("hex");
const git = (...args) => execFileSync("git", ["-C", source, ...args], { encoding: "utf8" }).trim();
const snapshot = await Promise.all(files.map(async (file) => {
  const content = await readFile(resolve(source, sourcePath(file)));
  return { file, content, sha256: hash(content) };
}));

// Unknown files must be moved out explicitly, never silently removed.
async function existingFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const result = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...await existingFiles(path));
    else result.push(relative(destination, path).replaceAll("\\", "/"));
  }
  return result;
}
const extras = (await existingFiles(destination)).filter((file) => !files.includes(file));
if (extras.length) throw new Error(`Non-production files in prod-code: ${extras.join(", ")}`);

if (check) {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  if (JSON.stringify(Object.keys(manifest.files).sort()) !== JSON.stringify([...files].sort())) {
    throw new Error("Snapshot manifest file list differs");
  }
  for (const { file, content, sha256 } of snapshot) {
    const copy = await readFile(resolve(destination, file));
    if (!content.equals(copy) || manifest.files[file] !== sha256) {
      throw new Error(`Production snapshot differs: ${file}`);
    }
  }
  console.log(`Verified ${files.length} production files, byte-for-byte, against ${source}`);
} else {
  const manifest = {
    source: "../project",
    commit: git("rev-parse", "HEAD"),
    sourceStatus: git("status", "--porcelain", "--", ...files.map(sourcePath)),
    files: Object.fromEntries(snapshot.map(({ file, sha256 }) => [file, sha256])),
  };
  for (const { file, content } of snapshot) {
    await mkdir(dirname(resolve(destination, file)), { recursive: true });
    await writeFile(resolve(destination, file), content);
  }
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Copied ${files.length} production files from ${source}`);
}
