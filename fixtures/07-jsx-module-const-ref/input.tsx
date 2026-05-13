// Module-level `const M = "..."` referenced from a JSX
// `defaultMessage={M}` attribute. Both plugins must produce identical
// id-hashes after folding `M` to its literal value.
import { FormattedMessage } from "react-intl";

const M = "Hello";

export function X() {
  return <FormattedMessage defaultMessage={M} />;
}
