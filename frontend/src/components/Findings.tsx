import {
  Badge,
  Box,
  Checkbox,
  Code,
  Heading,
  HStack,
  Link,
  Stack,
  Text,
} from "@chakra-ui/react";
import { useState } from "react";
import { SECTIONS, SEVERITIES, type Finding, type Severity } from "../types";

/** Findings grouped by section, with severity filter toggles. */
export function Findings({ findings }: { findings: Finding[] }) {
  const [shown, setShown] = useState<Record<Severity, boolean>>({
    error: true,
    warning: true,
    compat: true,
    info: true,
  });
  const visible = findings.filter((f) => shown[f.severity]);

  return (
    <Stack gap="4">
      <HStack gap="4" wrap="wrap">
        {SEVERITIES.map(({ severity, label }) => (
          <Checkbox.Root
            key={severity}
            checked={shown[severity]}
            onCheckedChange={(e) =>
              setShown((s) => ({ ...s, [severity]: e.checked === true }))
            }
          >
            <Checkbox.HiddenInput aria-label={label} />
            <Checkbox.Control />
            <Checkbox.Label>{label}</Checkbox.Label>
          </Checkbox.Root>
        ))}
      </HStack>
      {SECTIONS.map(({ section, heading }) => {
        const items = visible.filter((f) => f.section === section);
        if (items.length === 0) return null;
        return (
          <Box key={section}>
            <Heading as="h3" size="md" mb="2">
              {heading}
            </Heading>
            <Stack gap="2">
              {items.map((f, i) => (
                <Box
                  key={`${f.id}-${i}`}
                  borderWidth="1px"
                  borderRadius="md"
                  p="3"
                >
                  <HStack gap="2" wrap="wrap">
                    <Badge
                      colorPalette={
                        SEVERITIES.find((s) => s.severity === f.severity)?.color
                      }
                    >
                      {f.severity}
                    </Badge>
                    <Code>{f.id}</Code>
                    <Link
                      href={f.reference}
                      target="_blank"
                      rel="noreferrer"
                      ms="auto"
                    >
                      reference
                    </Link>
                  </HStack>
                  <Text mt="1">{f.message}</Text>
                </Box>
              ))}
            </Stack>
          </Box>
        );
      })}
      {findings.length > 0 && visible.length === 0 && (
        <Text color="fg.muted">All findings are hidden by the filters.</Text>
      )}
      {findings.length === 0 && <Text color="fg.muted">No findings.</Text>}
    </Stack>
  );
}
