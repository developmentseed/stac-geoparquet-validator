import { ChakraProvider, defaultSystem } from "@chakra-ui/react";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import { upload, validate } from "./api";
import type { Report } from "./types";

vi.mock("./api", () => ({
  upload: vi.fn(),
  validate: vi.fn(),
  docsUrl: () => "/api/docs",
}));

const UPLOADED = "s3://uploads-bucket/uploads/1234/items.parquet";

function report(href: string): Report {
  return {
    report_version: "1",
    href,
    summary: {
      parquet_valid: true,
      geoparquet_valid: true,
      geoparquet_version: "1.1.0",
      native_geospatial_types: false,
      stac_geoparquet_valid: true,
      counts: { error: 0, warning: 0, compat: 0, info: 0 },
    },
    findings: [],
  };
}

beforeEach(() => {
  window.history.replaceState(null, "", "/");
  vi.mocked(upload).mockResolvedValue(UPLOADED);
  vi.mocked(validate).mockImplementation(async (href) => report(href));
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function renderApp() {
  const { container } = render(
    <ChakraProvider value={defaultSystem}>
      <App />
    </ChakraProvider>,
  );
  return container.querySelector<HTMLInputElement>('input[type="file"]')!;
}

async function uploadFile() {
  const input = renderApp();
  const file = new File(["data"], "items.parquet");
  fireEvent.change(input, { target: { files: [file] } });
  await waitFor(() => expect(validate).toHaveBeenCalledWith(UPLOADED));
}

it("shows the uploaded file name, not the s3 href", async () => {
  await uploadFile();
  await waitFor(() =>
    expect(screen.getByLabelText("href")).toHaveProperty(
      "value",
      "items.parquet",
    ),
  );
  expect(window.location.search).toBe("");
});

it("revalidates the uploaded file when the name is resubmitted", async () => {
  await uploadFile();
  const validateButton = screen.getByRole("button", { name: /validate/i });
  await waitFor(() => expect(validateButton).toHaveProperty("disabled", false));
  fireEvent.click(validateButton);
  await waitFor(() => expect(validate).toHaveBeenCalledTimes(2));
  expect(validate).toHaveBeenLastCalledWith(UPLOADED);
  expect(window.location.search).toBe("");
});

it("validates a typed href after an upload", async () => {
  await uploadFile();
  const href = "https://example.com/other.parquet";
  fireEvent.change(screen.getByLabelText("href"), { target: { value: href } });
  fireEvent.click(screen.getByRole("button", { name: /validate/i }));
  await waitFor(() => expect(validate).toHaveBeenLastCalledWith(href));
  expect(new URLSearchParams(window.location.search).get("href")).toBe(href);
});

it("links to the API docs", () => {
  renderApp();
  expect(screen.getByRole("link", { name: "API" }).getAttribute("href")).toBe(
    "/api/docs",
  );
});
