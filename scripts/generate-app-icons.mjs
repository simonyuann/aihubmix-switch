import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, copyFileSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

// Keep the original brand artwork intact; a vector clip gives every generated
// platform icon the same transparent rounded corners.
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const artwork = readFileSync(join(root, "src/assets/icons/aihubmix-a.png")).toString("base64");
const temp = mkdtempSync(join(tmpdir(), "aihubmix-app-icons-"));
const output = join(temp, "icons");
const macOutput = join(temp, "mac-icons");
function sourceSvg(inset) {
  const size = 1024 - inset * 2;
  const radius = size * 224 / 1024;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><defs><clipPath id="tile"><rect x="${inset}" y="${inset}" width="${size}" height="${size}" rx="${radius}"/></clipPath></defs><image x="${inset}" y="${inset}" width="${size}" height="${size}" clip-path="url(#tile)" href="data:image/png;base64,${artwork}"/></svg>`;
}
// Tauri produces equivalent ICNS chunks in nondeterministic order. Sort the
// container records so repeated generation is byte-for-byte reproducible.
function canonicalIcns(path) {
  const bytes = readFileSync(path);
  if (bytes.toString("ascii", 0, 4) !== "icns" || bytes.readUInt32BE(4) !== bytes.length) throw new Error("Invalid ICNS container");
  const chunks = [];
  for (let pos = 8; pos < bytes.length;) {
    if (pos + 8 > bytes.length) throw new Error("Invalid ICNS record");
    const size = bytes.readUInt32BE(pos + 4);
    if (size < 8 || pos + size > bytes.length) throw new Error("Invalid ICNS record size");
    chunks.push(bytes.subarray(pos, pos + size));
    pos += size;
  }
  chunks.sort((a, b) => Buffer.compare(a.subarray(0, 4), b.subarray(0, 4)));
  writeFileSync(path, Buffer.concat([bytes.subarray(0, 8), ...chunks]));
}
const files = [
  "32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico", "icon.icns",
  "Square30x30Logo.png", "Square44x44Logo.png", "Square71x71Logo.png", "Square89x89Logo.png",
  "Square107x107Logo.png", "Square142x142Logo.png", "Square150x150Logo.png", "Square284x284Logo.png",
  "Square310x310Logo.png", "StoreLogo.png",
];
try {
  const require = createRequire(import.meta.url);
  const cli = require.resolve("@tauri-apps/cli/tauri.js");
  for (const [inset, destination] of [[0, output], [102, macOutput]]) {
    const source = join(temp, `source-${inset}.svg`);
    writeFileSync(source, sourceSvg(inset));
    execFileSync(process.execPath, [cli, "icon", source, "--output", destination], { cwd: root, stdio: "inherit" });
  }
  canonicalIcns(join(macOutput, "icon.icns"));
  for (const file of files) if (!existsSync(join(output, file))) throw new Error(`Icon generator did not produce ${file}`);
  if (!existsSync(join(macOutput, "icon.icns"))) throw new Error("Icon generator did not produce the macOS icon");
  for (const file of files) copyFileSync(join(file === "icon.icns" ? macOutput : output, file), join(root, "src-tauri/icons", file));
} finally {
  rmSync(temp, { recursive: true, force: true });
}
