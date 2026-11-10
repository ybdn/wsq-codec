// Packs the built npm package, installs the tarball in a scratch project and uses it from Node.js
// as an application would: through ESM `import` and through CommonJS `require`.
//
//   node wasm/scripts/check-package.mjs wasm/pkg wasm/demo/sample.wsq
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const [pkgDir, sampleFile] = process.argv.slice(2).map((p) => resolve(p));
if (!pkgDir || !sampleFile) {
  console.error("usage: check-package.mjs PKG_DIR SAMPLE.wsq");
  process.exit(64);
}

const scratch = mkdtempSync(join(tmpdir(), "wsq-package-"));
try {
  const run = (cwd, ...args) => execFileSync("npm", args, { cwd, encoding: "utf8" });
  run(pkgDir, "pack", "--pack-destination", scratch);
  const tarball = readdirSync(scratch).find((f) => f.endsWith(".tgz"));
  writeFileSync(join(scratch, "package.json"), '{ "name": "check", "version": "1.0.0", "private": true }');
  run(scratch, "install", "--no-audit", "--no-fund", join(scratch, tarball));

  const esm = `
    import { decode, readInfo, encode, version } from "wsq-codec";
    import { readFileSync } from "node:fs";
    const wsq = new Uint8Array(readFileSync(${JSON.stringify(sampleFile)}));
    const info = readInfo(wsq);
    const image = decode(wsq);
    if (image.pixels.length !== info.width * info.height) throw new Error("pixel count");
    for (const profile of ["conformant", "optimized"]) {
      const out = encode(image.pixels, image.width, image.height, { bitrate: 1, ppi: 500, profile });
      const again = decode(out);
      if (again.width !== info.width || again.ppi !== 500) throw new Error("round trip " + profile);
    }
    let message;
    try { decode(new Uint8Array(4)); } catch (e) { message = e instanceof Error ? e.message : "not an Error"; }
    if (!message || message === "not an Error") throw new Error("decode of garbage did not throw an Error");
    console.log("ESM ok, version", version(), "-", info.width + "x" + info.height);
  `;
  writeFileSync(join(scratch, "check.mjs"), esm);
  execFileSync("node", ["check.mjs"], { cwd: scratch, stdio: "inherit" });

  const cjs = `
    const { readInfo } = require("wsq-codec");
    const info = readInfo(new Uint8Array(require("node:fs").readFileSync(${JSON.stringify(sampleFile)})));
    if (!(info.width > 0)) throw new Error("readInfo");
    console.log("CommonJS ok");
  `;
  writeFileSync(join(scratch, "check.cjs"), cjs);
  execFileSync("node", ["check.cjs"], { cwd: scratch, stdio: "inherit" });

  const manifest = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf8"));
  console.log(`${manifest.name}@${manifest.version} loads from its tarball`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
