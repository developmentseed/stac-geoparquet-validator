import { afterEach, describe, expect, it, vi } from "vitest";
import { checks, upload, validate } from "./api";

afterEach(() => {
  vi.unstubAllGlobals();
});

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("api", () => {
  it("validate posts href", async () => {
    const report = { report_version: "1", href: "https://x/y.parquet" };
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(report));
    vi.stubGlobal("fetch", fetchMock);

    const result = await validate("https://x/y.parquet");

    expect(result).toEqual(report);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/validate");
    expect(init.method).toBe("POST");
    expect(init.body).toBe(JSON.stringify({ href: "https://x/y.parquet" }));
  });

  it("checks gets the catalog as json", async () => {
    const catalog = { sections: [], checks: [], retired: [] };
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(catalog));
    vi.stubGlobal("fetch", fetchMock);

    expect(await checks()).toEqual(catalog);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/checks");
    expect(init.headers.accept).toBe("application/json");
  });

  it("checks throws on an error response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(jsonResponse({ error: "bad" }, 500)),
    );

    await expect(checks()).rejects.toThrow();
  });

  it("validate throws on an error response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(jsonResponse({ error: "bad" }, 400)),
    );

    await expect(validate("nope")).rejects.toThrow();
  });

  it("upload puts file then returns href", async () => {
    const file = new File(["hello"], "items.parquet");
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({ upload_url: "https://s3/put", href: "s3://b/k" }),
      )
      .mockResolvedValueOnce(new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const href = await upload(file);

    expect(href).toBe("s3://b/k");
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/uploads");
    expect(init.body).toBe(
      JSON.stringify({ filename: "items.parquet", size: 5 }),
    );
    const [putUrl, putInit] = fetchMock.mock.calls[1];
    expect(putUrl).toBe("https://s3/put");
    expect(putInit.method).toBe("PUT");
    expect(putInit.body).toBe(file);
  });

  it("upload throws when the PUT fails", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(
          jsonResponse({ upload_url: "https://s3/put", href: "s3://b/k" }),
        )
        .mockResolvedValueOnce(new Response(null, { status: 403 })),
    );

    await expect(upload(new File(["x"], "a.parquet"))).rejects.toThrow();
  });
});
