// Re-prints JS/TS source through @swc/core's parser+printer.
//
// Why: babel and swc disagree on cosmetic output (quote style, semicolons,
// indentation, trailing commas). To compare *semantic* parity we feed both
// the babel and swc outputs through swc's printer and compare the result.
//
// Anything that survives this round-trip and still differs is a real
// difference in what the plugin emitted -> hard parity failure.

import swc from "@swc/core";

/**
 * @param {string} code
 * @param {string} filename (used only to pick the parser dialect)
 */
export async function normalize(code, filename) {
  const isTs = /\.tsx?$/.test(filename);
  const isTsx = /\.tsx$/.test(filename);
  const isJsx = /\.jsx$/.test(filename);
  const result = await swc.transform(code, {
    filename,
    isModule: "unknown",
    jsc: {
      parser: isTs
        ? { syntax: "typescript", tsx: isTsx }
        : { syntax: "ecmascript", jsx: isJsx },
      target: "es2022",
      // No plugins -- this is a pure pretty-print pass.
    },
    minify: false,
  });
  return result.code;
}
