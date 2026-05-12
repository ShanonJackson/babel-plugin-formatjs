// Transforms a fixture's `input.*` with babel-plugin-formatjs@10.5.41.
// Writes the raw babel output to `out-babel.<ext>` and the captured messages
// to `out-babel.messages.json` (via onMsgExtracted/onMetaExtracted).
//
// Usage: node scripts/run-babel.mjs <fixtureDir>

import { readFile, writeFile, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, extname } from "node:path";
import babel from "@babel/core";
import formatjsPlugin from "babel-plugin-formatjs";

const fixtureDir = process.argv[2];
if (!fixtureDir) {
  console.error("usage: run-babel.mjs <fixtureDir>");
  process.exit(2);
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
const { expectError: _expectError, ...pluginOpts } = rawOpts;

const source = await readFile(inputPath, "utf8");
const messages = [];
const meta = {};

const result = await babel.transformAsync(source, {
  filename: inputPath,
  babelrc: false,
  configFile: false,
  ast: false,
  sourceMaps: false,
  parserOpts: {
    sourceType: "module",
    plugins: ["jsx", "typescript"],
  },
  plugins: [
    [
      formatjsPlugin,
      {
        ...pluginOpts,
        onMsgExtracted(_file, msgs) {
          messages.push(...msgs);
        },
        onMetaExtracted(_file, m) {
          Object.assign(meta, m);
        },
      },
    ],
  ],
});

await writeFile(join(fixtureDir, `out-babel${ext}`), result.code, "utf8");
await writeFile(
  join(fixtureDir, "out-babel.messages.json"),
  JSON.stringify({ messages, meta }, null, 2),
  "utf8",
);
console.log(`babel: wrote out-babel${ext} (${messages.length} messages)`);
