import { FormattedMessage } from "react-intl";

export function Footer() {
  return (
    <FormattedMessage
      id="footer.copy"
      defaultMessage="© 2026 Acme, Inc."
      description="Footer copyright line"
    />
  );
}
