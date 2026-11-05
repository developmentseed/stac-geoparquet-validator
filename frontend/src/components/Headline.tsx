import { Badge, Box, HStack, SimpleGrid, Text } from "@chakra-ui/react";
import { LuCircleCheck, LuCircleX } from "react-icons/lu";
import { SEVERITIES, type Summary } from "../types";
import { SeverityIcon } from "./SeverityIcon";

function versionAnswer(summary: Summary): string {
  if (summary.geoparquet_version) return summary.geoparquet_version;
  if (summary.native_geospatial_types) return "native types, no geo metadata";
  return "—";
}

function Answer({
  id,
  label,
  value,
  ok,
}: {
  id: string;
  label: string;
  value: string;
  ok?: boolean;
}) {
  const color = ok === undefined ? undefined : ok ? "green.fg" : "red.fg";
  return (
    <Box data-testid={`answer-${id}`} borderWidth="1px" borderRadius="md" p="3">
      <Text fontSize="sm" color="fg.muted">
        {label}
      </Text>
      <HStack gap="2" color={color}>
        {ok === true && <LuCircleCheck aria-hidden />}
        {ok === false && <LuCircleX aria-hidden />}
        <Text fontSize="lg" fontWeight="semibold">
          {value}
        </Text>
      </HStack>
    </Box>
  );
}

/** The four headline answers plus counts by severity. */
export function Headline({ summary }: { summary: Summary }) {
  const yesNo = (v: boolean) => (v ? "Yes" : "No");
  return (
    <Box>
      <SimpleGrid columns={{ base: 1, sm: 2, md: 4 }} gap="3">
        <Answer
          id="parquet"
          label="Parquet valid"
          value={yesNo(summary.parquet_valid)}
          ok={summary.parquet_valid}
        />
        <Answer
          id="geoparquet"
          label="GeoParquet valid"
          value={yesNo(summary.geoparquet_valid)}
          ok={summary.geoparquet_valid}
        />
        <Answer
          id="version"
          label="GeoParquet version"
          value={versionAnswer(summary)}
        />
        <Answer
          id="stac"
          label="stac-geoparquet valid"
          value={yesNo(summary.stac_geoparquet_valid)}
          ok={summary.stac_geoparquet_valid}
        />
      </SimpleGrid>
      <HStack mt="3" gap="2" wrap="wrap">
        {SEVERITIES.map(({ severity, label, color }) => (
          <Badge key={severity} colorPalette={color} size="lg">
            <SeverityIcon severity={severity} />
            {summary.counts[severity]} {label}
          </Badge>
        ))}
      </HStack>
    </Box>
  );
}
