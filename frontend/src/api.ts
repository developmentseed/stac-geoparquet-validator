import type { Report } from "./types";

const base = (): string => import.meta.env.VITE_API_BASE ?? "";

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${base()}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    let message = `request failed with status ${response.status}`;
    try {
      const payload = await response.json();
      if (typeof payload?.error === "string") message = payload.error;
    } catch {}
    throw new Error(message);
  }
  return (await response.json()) as T;
}

/** Validates the file at `href` and returns the report. */
export function validate(href: string): Promise<Report> {
  return postJson<Report>("/api/validate", { href });
}

/**
 * Uploads `file` via a presigned URL and returns the `s3://` href to validate.
 * The browser sets `Content-Length` from the file body.
 */
export async function upload(file: File): Promise<string> {
  const { upload_url, href } = await postJson<{
    upload_url: string;
    href: string;
  }>("/api/uploads", { filename: file.name, size: file.size });
  const response = await fetch(upload_url, { method: "PUT", body: file });
  if (!response.ok) {
    throw new Error(`upload failed with status ${response.status}`);
  }
  return href;
}
