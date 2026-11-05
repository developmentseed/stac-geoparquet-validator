import {
  Alert,
  Container,
  Heading,
  HStack,
  Link,
  Spinner,
  Stack,
  Text,
} from "@chakra-ui/react";
import { useEffect, useRef, useState } from "react";
import { upload, validate } from "./api";
import { HrefForm } from "./components/HrefForm";
import { Report } from "./components/Report";
import { UploadButton } from "./components/UploadButton";
import type { Report as ReportData } from "./types";

type Status = "idle" | "uploading" | "validating";

/** The validator single-page app. */
export default function App() {
  const initial = new URLSearchParams(window.location.search).get("href") ?? "";
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ReportData | null>(null);
  const [formKey, setFormKey] = useState(0);
  const [formValue, setFormValue] = useState(initial);
  const started = useRef(false);

  const run = async (href: string) => {
    setStatus("validating");
    setError(null);
    setReport(null);
    try {
      setReport(await validate(href));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStatus("idle");
    }
  };

  const submitHref = (href: string) => {
    const url = new URL(window.location.href);
    url.searchParams.set("href", href);
    window.history.replaceState(null, "", url);
    void run(href);
  };

  const submitFile = async (file: File) => {
    setStatus("uploading");
    setError(null);
    setReport(null);
    try {
      const href = await upload(file);
      const url = new URL(window.location.href);
      url.searchParams.delete("href");
      window.history.replaceState(null, "", url);
      setFormValue(href);
      setFormKey((k) => k + 1);
      await run(href);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("idle");
    }
  };

  useEffect(() => {
    if (initial && !started.current) {
      started.current = true;
      void run(initial);
    }
  }, [initial]);

  const busy = status !== "idle";
  return (
    <Container maxW="4xl" py="8">
      <Stack gap="6">
        <Stack>
          <Heading size="2xl">stac-geoparquet validator</Heading>
          <Text color="fg.muted">
            Validates{" "}
            <Link href="https://stac-geoparquet.org/">stac-geoparquet</Link>{" "}
            files. Find a bug or have a feature request? Open an{" "}
            <Link href="https://github.com/developmentseed/stac-geoparquet-validator/issues">
              issue on Github
            </Link>
            .
          </Text>
        </Stack>
        <HStack align="start">
          <HrefForm
            key={formKey}
            initial={formValue}
            disabled={busy}
            onSubmit={submitHref}
          />
          <UploadButton disabled={busy} onFile={submitFile} />
        </HStack>
        {busy && (
          <HStack>
            <Spinner />
            <Text>{status === "uploading" ? "Uploading…" : "Validating…"}</Text>
          </HStack>
        )}
        {error && (
          <Alert.Root status="error">
            <Alert.Indicator />
            <Alert.Title>{error}</Alert.Title>
          </Alert.Root>
        )}
        {report && <Report report={report} />}
      </Stack>
    </Container>
  );
}
