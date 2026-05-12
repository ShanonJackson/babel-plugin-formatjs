import { useIntl } from "react-intl";
const base = { description: "x" };
export function X() {
  return useIntl().formatMessage({
    ...base,
    defaultMessage: "hi",
  });
}
