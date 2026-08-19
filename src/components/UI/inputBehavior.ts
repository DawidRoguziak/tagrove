import type { InputHTMLAttributes } from "react";
type BrowserAssistDisabledProps = Pick<
  InputHTMLAttributes<HTMLInputElement>,
  "autoCapitalize" | "autoComplete" | "autoCorrect" | "spellCheck"
>;
export const browserAssistDisabledProps = {
  autoCapitalize: "none",
  autoComplete: "off",
  autoCorrect: "off",
  spellCheck: false
} satisfies BrowserAssistDisabledProps;