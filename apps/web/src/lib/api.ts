/** Typed client for the Directory Worker API (`/api/v1`). */

export interface HealthCheck {
  name: string;
  status: "ok" | "degraded";
  detail?: string;
}

export interface HealthReport {
  status: "ok" | "degraded";
  service: string;
  version: string;
  checks: HealthCheck[];
}

export interface ObjectInfo {
  key: string;
  size: number;
  content_type: string | null;
  uploaded_at: string | null;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
  ) {
    super(`${status} ${code}`);
  }
}

type Fetch = typeof fetch;

/** Empty = same origin. Native clients can point this at the deployed Worker. */
const BASE = (import.meta.env?.VITE_API_BASE as string | undefined) ?? "";

/** Encode each path segment so keys like `a/b c.txt` survive the URL. */
export function objectPath(key: string): string {
  return `/api/v1/storage/objects/${key.split("/").map(encodeURIComponent).join("/")}`;
}

async function fail(res: Response): Promise<never> {
  let code = "request_failed";
  try {
    code = ((await res.json()) as { error?: string }).error ?? code;
  } catch {
    /* non-JSON body */
  }
  throw new ApiError(res.status, code);
}

export async function fetchHealth(fetchImpl: Fetch = fetch): Promise<HealthReport> {
  const res = await fetchImpl(`${BASE}/api/v1/health`, { headers: { Accept: "application/json" } });
  // 503 still carries a report describing which check is degraded.
  if (res.ok || res.status === 503) {
    const body = (await res.json()) as Partial<HealthReport>;
    if (body.service === "directory" && Array.isArray(body.checks)) return body as HealthReport;
  }
  return fail(res);
}

export class StorageClient {
  constructor(
    private readonly token: string,
    private readonly fetchImpl: Fetch = fetch,
  ) {}

  private request(path: string, init: RequestInit = {}) {
    const headers = new Headers(init.headers);
    headers.set("Authorization", `Bearer ${this.token}`);
    return this.fetchImpl(`${BASE}${path}`, { ...init, headers });
  }

  async list(prefix?: string): Promise<ObjectInfo[]> {
    const q = prefix ? `?prefix=${encodeURIComponent(prefix)}` : "";
    const res = await this.request(`/api/v1/storage/objects${q}`);
    if (!res.ok) return fail(res);
    return ((await res.json()) as { objects: ObjectInfo[] }).objects;
  }

  async upload(key: string, body: Blob): Promise<ObjectInfo> {
    const res = await this.request(objectPath(key), {
      method: "PUT",
      body,
      headers: { "Content-Type": body.type || "application/octet-stream" },
    });
    if (!res.ok) return fail(res);
    return (await res.json()) as ObjectInfo;
  }

  async download(key: string): Promise<Blob> {
    const res = await this.request(objectPath(key));
    if (!res.ok) return fail(res);
    return res.blob();
  }

  async remove(key: string): Promise<void> {
    const res = await this.request(objectPath(key), { method: "DELETE" });
    if (!res.ok) return fail(res);
  }
}

/** Turn a file name into a valid storage key (see core `ObjectKey`). */
export function toObjectKey(fileName: string, folder = "uploads"): string {
  const safe = fileName
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "") // drop accents: é → e
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^[-.]+|-+$/g, "")
    .slice(0, 200);
  return `${folder}/${safe || "file"}`;
}
