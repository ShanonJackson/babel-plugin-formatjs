import { FormattedMessage } from "react-intl";

export function Banner() {
  return (
    <FormattedMessage
      defaultMessage="Welcome back, {name}."
      description="Returning-user banner"
    />
  );
}
