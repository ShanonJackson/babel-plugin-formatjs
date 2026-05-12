import { useIntl } from "react-intl";

export function Greeting() {
  const intl = useIntl();
  return intl.formatMessage({
    defaultMessage: "Hello, {name}!",
    description: "Greets the user by name",
  });
}
