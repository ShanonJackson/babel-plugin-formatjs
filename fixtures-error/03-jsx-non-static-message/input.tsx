// Function call in `defaultMessage` — Option 1 folder doesn't evaluate
// function calls, must still hard-error.
import { FormattedMessage } from "react-intl";

declare function getMessage(): string;

export function X() {
  return <FormattedMessage defaultMessage={getMessage()} />;
}
