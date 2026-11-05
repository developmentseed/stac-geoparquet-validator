import type { IconType } from "react-icons";
import { LuCircleX, LuInfo, LuTriangleAlert, LuUnplug } from "react-icons/lu";
import type { Severity } from "../types";

const ICONS: Record<Severity, IconType> = {
  error: LuCircleX,
  warning: LuTriangleAlert,
  compat: LuUnplug,
  info: LuInfo,
};

/** The icon for a finding severity. */
export function SeverityIcon({ severity }: { severity: Severity }) {
  const Icon = ICONS[severity];
  return <Icon aria-hidden />;
}
