// `intl["formatMessage"](...)` — computed-member callee with a string-literal
// accessor. Babel does NOT extract from this (its matcher checks
// `property.isIdentifier(...)`, which returns false for a StringLiteral).
// The new Rust port must silently leave this alone too — NOT hard-error.
import { useIntl } from "react-intl";

export function X() {
  const intl = useIntl();
  return intl["formatMessage"]({
    defaultMessage: "Hi",
    description: "greeting",
  });
}
