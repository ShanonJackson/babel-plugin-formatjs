// Demonstrates the Option 1 evaluator: a module-level `const` plus a
// `+`-concat in `defaultMessage` must produce the SAME id-hash that babel
// produces by folding the expression via `path.evaluate()`.
import { useIntl } from "react-intl";

const PIECE = "world";

export function X() {
  return useIntl().formatMessage({
    defaultMessage: "Hello, " + PIECE + "!",
  });
}
