import { FormattedMessage, defineMessages } from "react-intl";

const messages = defineMessages({
  move: {
    defaultMessage: "Move",
    description: "Move button label",
  },
  cancel: {
    defaultMessage: "Cancel",
    description: "Cancel button label",
  },
});

export function Toolbar() {
  return (
    <>
      <FormattedMessage {...messages.move} />
      <FormattedMessage {...messages.cancel} />
    </>
  );
}
