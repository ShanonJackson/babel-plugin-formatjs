// Parity driver.
//
//   `fixtures/`        — each subdir's babel output (after swc-codegen
//                        normalization) MUST equal its swc output, byte-for-byte.
//   `fixtures-error/`  — each subdir's `options.json` declares an `expectError`
//                        substring; the swc plugin MUST fail with a matching
//                        message. (Babel is not run for these — they test our
//                        hard-error policy, not parity.)
//
// Any mismatch is a hard failure: the new Rust plugin diverged from its spec.

import { readdir, readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { join, extname } from "node:path";
import { existsSync } from "node:fs";
import { normalize } from "./normalize.mjs";

let failed = 0;
let total = 0;

// =====================================================================
// Pass 1: parity fixtures
// =====================================================================
const parityDirs = (await readdir("fixtures", { withFileTypes: true }))
  .filter((d) => d.isDirectory())
  .map((d) => join("fixtures", d.name));

for (const dir of parityDirs) {
  total++;
  const files = await readdir(dir);
  const inputName = files.find((f) => /^input\.(m?[jt]sx?)$/.test(f));
  if (!inputName) {
    console.log(`SKIP  ${dir} (no input.*)`);
    continue;
  }
  const ext = extname(inputName);

  const runBabel = spawnSync("node", ["scripts/run-babel.mjs", dir], {
    stdio: "inherit",
  });
  if (runBabel.status !== 0) {
    failed++;
    console.log(`FAIL  ${dir} (babel run failed)`);
    continue;
  }
  const runSwc = spawnSync("node", ["scripts/run-swc.mjs", dir], {
    stdio: "inherit",
  });
  if (runSwc.status !== 0) {
    failed++;
    console.log(`FAIL  ${dir} (swc run failed)`);
    continue;
  }

  const babelOut = await readFile(join(dir, `out-babel${ext}`), "utf8");
  const swcOut = await readFile(join(dir, `out-swc${ext}`), "utf8");
  const [babelNorm, swcNorm] = await Promise.all([
    normalize(babelOut, inputName),
    normalize(swcOut, inputName),
  ]);
  await writeFile(join(dir, `out-babel-normalized${ext}`), babelNorm, "utf8");

  if (babelNorm === swcNorm) {
    console.log(`PASS  ${dir}`);
  } else {
    failed++;
    console.log(`FAIL  ${dir}`);
    console.log("--- babel (normalized) ---");
    console.log(babelNorm);
    console.log("--- swc ---");
    console.log(swcNorm);
  }
}

// =====================================================================
// Pass 2: error fixtures
// =====================================================================
const errorRoot = "fixtures-error";
if (existsSync(errorRoot)) {
  const errorDirs = (await readdir(errorRoot, { withFileTypes: true }))
    .filter((d) => d.isDirectory())
    .map((d) => join(errorRoot, d.name));

  for (const dir of errorDirs) {
    total++;
    const optsPath = join(dir, "options.json");
    const opts = existsSync(optsPath)
      ? JSON.parse(await readFile(optsPath, "utf8"))
      : {};
    const expectError = opts.expectError;
    if (!expectError) {
      failed++;
      console.log(`FAIL  ${dir} (missing options.expectError)`);
      continue;
    }

    const runSwc = spawnSync("node", ["scripts/run-swc.mjs", dir], {
      stdio: "pipe",
      encoding: "utf8",
    });
    const combined = (runSwc.stdout || "") + (runSwc.stderr || "");

    if (runSwc.status === 0) {
      failed++;
      console.log(`FAIL  ${dir} (swc plugin should have errored, but exited 0)`);
      continue;
    }
    if (!combined.includes(expectError)) {
      failed++;
      console.log(
        `FAIL  ${dir} (plugin errored, but message did not contain ${JSON.stringify(
          expectError,
        )})`,
      );
      console.log("--- swc stderr ---");
      console.log(combined.slice(0, 2000));
      continue;
    }
    console.log(`PASS  ${dir} (errored with expected message)`);
  }
}

if (failed) {
  console.error(`\n${failed}/${total} fixture(s) failed parity`);
  process.exit(1);
} else {
  console.log(`\nall ${total} fixtures passed`);
}
