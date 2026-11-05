import { Stack } from "@chakra-ui/react";
import type { Report as ReportData } from "../types";
import { Findings } from "./Findings";
import { Headline } from "./Headline";

/** The full validation report: headline answers and grouped findings. */
export function Report({ report }: { report: ReportData }) {
  return (
    <Stack gap="5">
      <Headline summary={report.summary} />
      <Findings findings={report.findings} />
    </Stack>
  );
}
