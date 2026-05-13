// Template literal with `${...}` interpolation referencing a module-level
// const. Folder must interleave quasis and folded exprs identically to
// babel's `path.evaluate()`.
import { useIntl } from "react-intl";

const BUTTON = "Save";

export function X() {
  return useIntl().formatMessage({
    defaultMessage: `Click the ${BUTTON} button to continue`,
  });
}
