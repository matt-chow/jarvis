import { describe, expect, it, vi } from "vitest";
import { Api, ApiError, type HealthReport } from "./api";

const report: HealthReport = {
  status: "degraded",
  service: "directory",
  version: "0.0.1",
  checks: [{ name: "database", status: "degraded", detail: "migrations not applied" }],
};

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

type Call = [string | URL | Request, RequestInit | undefined];

function recorder(respond: () => Response) {
  const calls: Call[] = [];
  const fetchImpl = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
    calls.push([url, init]);
    return respond();
  });
  return { calls, fetchImpl };
}

describe("health", () => {
  it("accepts a degraded (503) report", async () => {
    const { fetchImpl, calls } = recorder(() => json(503, report));
    await expect(new Api(fetchImpl).health()).resolves.toEqual(report);
    expect(calls[0][0]).toBe("/api/v1/health");
  });

  it("rejects non-health responses", async () => {
    await expect(new Api(async () => json(404, { error: "not_found" })).health()).rejects.toMatchObject({
      status: 404,
      code: "not_found",
    });
    await expect(new Api(async () => new Response("<html>", { status: 200 })).health()).rejects.toBeInstanceOf(Error);
  });
});

describe("requests", () => {
  it("sends JSON with same-origin credentials", async () => {
    const { fetchImpl, calls } = recorder(() => json(201, { token: { id: "t1" }, secret: "dir_x" }));
    await new Api(fetchImpl).createToken("Phone");
    const [url, init] = calls[0];
    expect(url).toBe("/api/v1/auth/tokens");
    expect(init?.method).toBe("POST");
    expect(init?.credentials).toBe("same-origin");
    expect(new Headers(init?.headers).get("Content-Type")).toBe("application/json");
    expect(JSON.parse(String(init?.body))).toEqual({ name: "Phone" });
  });

  it("uses a bearer token when given (native clients)", async () => {
    const { fetchImpl, calls } = recorder(() => json(200, { folder: "docs", folders: [], files: [] }));
    await new Api(fetchImpl, "https://example.test", "dir_secret").files("docs");
    const [url, init] = calls[0];
    expect(url).toBe("https://example.test/api/v1/files?folder=docs");
    expect(new Headers(init?.headers).get("Authorization")).toBe("Bearer dir_secret");
  });

  it("handles 204 and surfaces API error codes", async () => {
    await expect(new Api(async () => new Response(null, { status: 204 })).logout()).resolves.toBeUndefined();
    const err = await new Api(async () => json(409, { error: "last_passkey" }))
      .deletePasskey("abc")
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err).toMatchObject({ status: 409, code: "last_passkey" });
  });
});
