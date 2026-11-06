import type { components } from "./openapi";

type Schemas = components["schemas"];

export type Severity = Schemas["Severity"];
export type Section = Schemas["Section"];
export type Finding = Schemas["Finding"];
export type Counts = Schemas["Counts"];
export type Summary = Schemas["Summary"];
export type Report = Schemas["Report"];
export type Source = Schemas["Source"];
export type SectionInfo = Schemas["SectionInfo"];
export type CheckInfo = Schemas["CheckInfo"];
export type RetiredCheck = Schemas["RetiredCheck"];
export type Catalog = Schemas["Catalog"];
export type UploadResponse = Schemas["UploadResponse"];

export const SECTIONS: { section: Section; heading: string }[] = [
  { section: "parquet", heading: "Parquet" },
  { section: "geoparquet", heading: "GeoParquet" },
  {
    section: "geoparquet_best_practices",
    heading: "GeoParquet best practices",
  },
  { section: "stac_geoparquet", heading: "stac-geoparquet" },
  {
    section: "stac_geoparquet_best_practices",
    heading: "stac-geoparquet best practices",
  },
];

export const SEVERITIES: {
  severity: Severity;
  label: string;
  color: string;
}[] = [
  { severity: "error", label: "Error", color: "red" },
  { severity: "warning", label: "Warning", color: "orange" },
  { severity: "compat", label: "Compat", color: "purple" },
  { severity: "info", label: "Info", color: "blue" },
];
