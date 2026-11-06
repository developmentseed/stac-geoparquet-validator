export type Severity = "error" | "warning" | "compat" | "info";

export type Section =
  | "parquet"
  | "geoparquet"
  | "geoparquet_best_practices"
  | "stac_geoparquet"
  | "stac_geoparquet_best_practices";

export interface Finding {
  id: string;
  severity: Severity;
  section: Section;
  message: string;
  reference: string;
}

export interface Counts {
  error: number;
  warning: number;
  compat: number;
  info: number;
}

export interface Summary {
  parquet_valid: boolean;
  geoparquet_valid: boolean;
  geoparquet_version: string | null;
  native_geospatial_types: boolean;
  stac_geoparquet_valid: boolean;
  counts: Counts;
}

export interface Report {
  report_version: "1";
  href: string;
  summary: Summary;
  findings: Finding[];
}

export interface Source {
  title: string;
  href: string;
}

export interface SectionInfo {
  section: Section;
  title: string;
  source: Source;
}

export interface CheckInfo {
  id: string;
  severity: Severity;
  section: Section | null;
  sampled: boolean;
  description: string;
  references: string[];
}

export interface RetiredCheck {
  id: string;
  retired: string;
  reason: string;
}

export interface Catalog {
  sections: SectionInfo[];
  checks: CheckInfo[];
  retired: RetiredCheck[];
}

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
