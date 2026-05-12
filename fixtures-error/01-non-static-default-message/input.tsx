import { useIntl } from "react-intl";
const PIECE = "world";
export function X() {
  return useIntl().formatMessage({
    defaultMessage: "Hello, " + PIECE + "!",
  });
}
