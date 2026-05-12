// Transforms a fixture's `input.*` with the Rust/SWC plugin built from
// `crates/swc-plugin-formatjs`. Writes `out-swc.<ext>`.
//
// Expects the wasm artifact at:
//   target/wasm32-wasip1/release/swc_plugin_formatjs.wasm
//
// Build it with:
//   cargo build --release --target wasm32-wasip1 \
//     -p swc-plugin-formatjs --features plugin --no-default-features
//
// Usage: node scripts/run-swc.mjs <fixtureDir>

import { readFile, writeFile, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, extname, resolve } from "node:path";
import swc from "@swc/core";

const fixtureDir = process.argv[2];
if (!fixtureDir) {
  console.error("usage: run-swc.mjs <fixtureDir>");
  process.exit(2);
}

const wasmPath = resolve(
  "target/wasm32-wasip1/release/swc_plugin_formatjs.wasm",
);
if (!existsSync(wasmPath)) {
  console.error(
    `missing wasm plugin at ${wasmPath}.\n` +
      `build with: cargo build --release --target wasm32-wasip1 -p swc-plugin-formatjs --features plugin --no-default-features`,
  );
  process.exit(3);
}

const files = await readdir(fixtureDir);
const inputName = files.find((f) => /^input\.(m?[jt]sx?)$/.test(f));
if (!inputName) {
  console.error(`no input.* in ${fixtureDir}`);
  process.exit(2);
}
const inputPath = join(fixtureDir, inputName);
const ext = extname(inputName);

const optsPath = join(fixtureDir, "options.json");
const rawOpts = existsSync(optsPath)
  ? JSON.parse(await readFile(optsPath, "utf8"))
  : {};
// `expectError` is a harness-only directive (used by parity.mjs to assert
// the plugin throws). It is NOT a plugin option — strip it before forwarding.
const { expectError: _expectError, ...pluginOpts } = rawOpts;

const source = await readFile(inputPath, "utf8");

const isTsx = ext === ".tsx" || ext === ".ts";
const result = await swc.transform(source, {
  filename: inputPath,
  isModule: true,
  jsc: {
    parser: isTsx
      ? { syntax: "typescript", tsx: ext === ".tsx" }
      : { syntax: "ecmascript", jsx: ext === ".jsx" },
    target: "es2022",
    experimental: {
      plugins: [[wasmPath, pluginOpts]],
    },
  },
});

await writeFile(join(fixtureDir, `out-swc${ext}`), result.code, "utf8");
console.log(`swc: wrote out-swc${ext}`);
