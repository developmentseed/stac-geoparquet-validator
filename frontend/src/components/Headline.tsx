import { Badge, Box, HStack, SimpleGrid, Text } from "@chakra-ui/react";
import { SEVERITIES, type Summary } from "../types";

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
      <Text fontSize="lg" fontWeight="semibold" color={color}>
        {value}
      </Text>
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
            {summary.counts[severity]} {label}
          </Badge>
        ))}
      </HStack>
    </Box>
  );
}
