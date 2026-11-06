import {
  Alert,
  Badge,
  Box,
  Button,
  CloseButton,
  Code,
  Dialog,
  Heading,
  HStack,
  Link,
  Portal,
  Spinner,
  Stack,
  Text,
} from "@chakra-ui/react";
import { useState } from "react";
import { LuBookOpen, LuExternalLink } from "react-icons/lu";
import { checks, checksUrl } from "../api";
import { SEVERITIES, type Catalog } from "../types";
import { SeverityIcon } from "./SeverityIcon";

function InlineCode({ text }: { text: string }) {
  return (
    <>
      {text
        .split("`")
        .map((part, i) => (i % 2 === 1 ? <Code key={i}>{part}</Code> : part))}
    </>
  );
}

function label(href: string): string {
  const url = new URL(href);
  const file = url.pathname.split("/").pop() || url.hostname;
  return url.hash ? `${file}${url.hash}` : file;
}

function Rules({ catalog }: { catalog: Catalog }) {
  return (
    <Stack gap="6">
      <Text>
        Every rule cites an upstream specification or best-practice document.
        Errors are violations of a specification, warnings are deviations from a
        best practice, compat findings are valid files that some readers can't
        open, and info findings are facts about the file. Rules marked † read a
        sample of rows. Ids are stable; the full list is also available as{" "}
        <Link href={checksUrl()} target="_blank" rel="noreferrer">
          HTML, Markdown or JSON
        </Link>
        .
      </Text>
      {catalog.sections.map(({ section, title, source }) => {
        const rules = catalog.checks.filter((r) => r.section === section);
        if (rules.length === 0) return null;
        return (
          <Box key={section}>
            <Heading as="h3" size="md">
              {title}
            </Heading>
            <Text color="fg.muted" fontSize="sm" mb="2">
              From{" "}
              <Link href={source.href} target="_blank" rel="noreferrer">
                {source.title}
              </Link>
            </Text>
            <Stack gap="2">
              {rules.map((rule) => (
                <Box key={rule.id} borderWidth="1px" borderRadius="md" p="3">
                  <HStack gap="2" wrap="wrap">
                    <Badge
                      colorPalette={
                        SEVERITIES.find((s) => s.severity === rule.severity)
                          ?.color
                      }
                    >
                      <SeverityIcon severity={rule.severity} />
                      {rule.severity}
                    </Badge>
                    <Code>{rule.id}</Code>
                    {rule.sampled && (
                      <Text as="span" title="Reads a sample of rows">
                        †
                      </Text>
                    )}
                  </HStack>
                  <Text mt="1">
                    <InlineCode text={rule.description} />
                  </Text>
                  <HStack gap="3" wrap="wrap" mt="1" fontSize="sm">
                    {rule.references.map((href) => (
                      <Link
                        key={href}
                        href={href}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {label(href)} <LuExternalLink aria-hidden />
                      </Link>
                    ))}
                  </HStack>
                </Box>
              ))}
            </Stack>
          </Box>
        );
      })}
    </Stack>
  );
}

/** A button that opens a dialog listing every rule and its upstream source. */
export function RulesDialog() {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = async () => {
    if (catalog || loading) return;
    setLoading(true);
    setError(null);
    try {
      setCatalog(await checks());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog.Root
      size="xl"
      scrollBehavior="inside"
      onOpenChange={(e) => {
        if (e.open) void load();
      }}
    >
      <Dialog.Trigger asChild>
        <Button variant="outline" size="sm">
          <LuBookOpen aria-hidden /> Rules
        </Button>
      </Dialog.Trigger>
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>Rules</Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>
              {loading && <Spinner />}
              {error && (
                <Alert.Root status="error" role="alert">
                  <Alert.Indicator />
                  <Alert.Title>{error}</Alert.Title>
                </Alert.Root>
              )}
              {catalog && <Rules catalog={catalog} />}
            </Dialog.Body>
            <Dialog.CloseTrigger asChild>
              <CloseButton size="sm" />
            </Dialog.CloseTrigger>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
