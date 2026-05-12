import { useIntl } from "react-intl";
export function X() {
  return useIntl().formatMessage({
    defaultMessage: "Hello",
  });
}
