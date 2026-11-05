import { ChakraProvider, defaultSystem } from "@chakra-ui/react";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Report as ReportData } from "../types";
import { Report } from "./Report";

afterEach(cleanup);

const report: ReportData = {
  report_version: "1",
  href: "https://example.com/items.parquet",
  summary: {
    parquet_valid: true,
    geoparquet_valid: true,
    geoparquet_version: "1.1.0",
    native_geospatial_types: false,
    stac_geoparquet_valid: false,
    counts: { error: 1, warning: 1, compat: 0, info: 0 },
  },
  findings: [
    {
      id: "stac.column_type",
      severity: "error",
      section: "stac_geoparquet",
      message: "the datetime column has the wrong type",
      reference: "https://example.com/spec#columns",
    },
    {
      id: "bp.compression",
      severity: "warning",
      section: "geoparquet_best_practices",
      message: "use zstd compression",
      reference: "https://example.com/bp#compression",
    },
  ],
};

function renderReport(data: ReportData = report) {
  return render(
    <ChakraProvider value={defaultSystem}>
      <Report report={data} />
    </ChakraProvider>,
  );
}

describe("Report", () => {
  it("renders four headline answers", () => {
    renderReport();

    expect(screen.getByTestId("answer-parquet").textContent).toContain("Yes");
    expect(screen.getByTestId("answer-geoparquet").textContent).toContain(
      "Yes",
    );
    expect(screen.getByTestId("answer-version").textContent).toContain("1.1.0");
    expect(screen.getByTestId("answer-stac").textContent).toContain("No");
  });

  it("shows native types when there is no geo metadata", () => {
    renderReport({
      ...report,
      summary: {
        ...report.summary,
        geoparquet_version: null,
        native_geospatial_types: true,
      },
    });

    expect(screen.getByTestId("answer-version").textContent).toContain(
      "native types, no geo metadata",
    );
  });

  it("groups findings by section in order", () => {
    renderReport();

    const headings = screen
      .getAllByRole("heading", { level: 3 })
      .map((h) => h.textContent);
    expect(headings).toEqual(["GeoParquet best practices", "stac-geoparquet"]);
    const hrefs = screen
      .getAllByRole("link")
      .map((a) => a.getAttribute("href"));
    expect(hrefs).toEqual([
      "https://example.com/bp#compression",
      "https://example.com/spec#columns",
    ]);
  });

  it("severity filter hides warnings", async () => {
    renderReport();
    expect(screen.getByText("bp.compression")).toBeTruthy();

    fireEvent.click(screen.getByRole("checkbox", { name: "Warning" }));

    await waitFor(() =>
      expect(screen.queryByText("bp.compression")).toBeNull(),
    );
    expect(screen.getByText("stac.column_type")).toBeTruthy();
  });
});
