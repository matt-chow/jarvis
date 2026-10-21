import { describe, expect, it, vi } from "vitest";
import { ApiError, StorageClient, fetchHealth, objectPath, toObjectKey, type HealthReport } from "./api";

const report: HealthReport = {
  status: "degraded",
  service: "directory",
  version: "0.0.1",
  checks: [{ name: "database", status: "degraded", detail: "migrations not applied" }],
};

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

describe("fetchHealth", () => {
  it("accepts a degraded (503) report", async () => {
    const fetchImpl = vi.fn(async () => json(503, report));
    await expect(fetchHealth(fetchImpl)).resolves.toEqual(report);
    expect(fetchImpl).toHaveBeenCalledWith("/api/v1/health", expect.anything());
  });

  it("rejects non-health responses", async () => {
    await expect(fetchHealth(async () => json(404, { error: "not_found" }))).rejects.toMatchObject({
      status: 404,
      code: "not_found",
    });
    await expect(fetchHealth(async () => new Response("<html>", { status: 200 }))).rejects.toBeInstanceOf(Error);
  });
});

describe("StorageClient", () => {
  it("sends the bearer token and encodes keys", async () => {
    const fetchImpl = vi.fn(async (_url: string | URL | Request, _init?: RequestInit) => json(200, { objects: [] }));
    await new StorageClient("secret", fetchImpl).list("docs/");
    const [url, init] = fetchImpl.mock.calls[0];
    expect(url).toBe("/api/v1/storage/objects?prefix=docs%2F");
    expect(new Headers(init?.headers).get("Authorization")).toBe("Bearer secret");
  });

  it("surfaces API error codes", async () => {
    const client = new StorageClient("bad", async () => json(401, { error: "unauthorized" }));
    const err = await client.list().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ status: 401, code: "unauthorized" });
  });
});

describe("keys", () => {
  it("builds encoded object paths", () => {
    expect(objectPath("uploads/a b.txt")).toBe("/api/v1/storage/objects/uploads/a%20b.txt");
  });

  it("sanitizes file names into valid keys", () => {
    expect(toObjectKey("My Résumé (final).pdf")).toBe("uploads/My-Resume-final-.pdf");
    expect(toObjectKey("../../etc/passwd")).toBe("uploads/etc-passwd");
    expect(toObjectKey("???")).toBe("uploads/file");
    expect(toObjectKey("ok_name-1.txt")).toMatch(/^uploads\/[A-Za-z0-9._-]+$/);
  });
});
