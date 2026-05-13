// Function-local `const` — the Option 1 evaluator only collects
// module-level consts. This must still hard-error.
import { useIntl } from "react-intl";

export function X() {
  const PIECE = "world";
  return useIntl().formatMessage({
    defaultMessage: "Hello, " + PIECE + "!",
  });
}
