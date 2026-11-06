import { ChakraProvider, defaultSystem } from "@chakra-ui/react";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { checks } from "../api";
import type { Catalog } from "../types";
import { RulesDialog } from "./RulesDialog";

vi.mock("../api", () => ({
  checks: vi.fn(),
  checksUrl: () => "/api/checks",
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const catalog: Catalog = {
  sections: [
    {
      section: "geoparquet_best_practices",
      title: "GeoParquet best practices",
      source: {
        title: "Distributing GeoParquet",
        href: "https://example.com/distributing",
      },
    },
  ],
  checks: [
    {
      id: "bp.compression",
      severity: "warning",
      section: "geoparquet_best_practices",
      sampled: false,
      description: "Compression is not `zstd`.",
      references: ["https://example.com/distributing#compression"],
    },
    {
      id: "sample.skipped",
      severity: "info",
      section: null,
      sampled: false,
      description: "Skipped.",
      references: [],
    },
  ],
  retired: [],
};

function openDialog() {
  render(
    <ChakraProvider value={defaultSystem}>
      <RulesDialog />
    </ChakraProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /rules/i }));
  return screen.findByRole("dialog");
}

it("lists the rules and their sources when opened", async () => {
  vi.mocked(checks).mockResolvedValue(catalog);
  const dialog = await openDialog();
  expect(await within(dialog).findByText("bp.compression")).toBeTruthy();
  const hrefs = within(dialog)
    .getAllByRole("link")
    .map((a) => a.getAttribute("href"));
  expect(hrefs).toContain("https://example.com/distributing");
  expect(hrefs).toContain("https://example.com/distributing#compression");
  expect(within(dialog).queryByText("sample.skipped")).toBeNull();
  expect(checks).toHaveBeenCalledTimes(1);
});

it("shows an error when the rules can't be loaded", async () => {
  vi.mocked(checks).mockRejectedValue(new Error("boom"));
  const dialog = await openDialog();
  expect(await within(dialog).findByRole("alert")).toBeTruthy();
});
