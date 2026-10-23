/**
 * Minimal ePub reading without dependencies: an ePub is a zip (read here with
 * the browser's DecompressionStream) holding an OPF manifest, a spine of
 * XHTML chapters, and their images and styles. Chapters are shown in a
 * script-less sandboxed iframe; scripts are stripped as well.
 */

/** Guards against zip bombs and absurd files. */
const MAX_ENTRIES = 5_000;
const MAX_TOTAL_BYTES = 200 * 1024 * 1024;

export type Zip = Map<string, Uint8Array>;

function u16(v: DataView, o: number) {
  return v.getUint16(o, true);
}
function u32(v: DataView, o: number) {
  return v.getUint32(o, true);
}

async function inflateRaw(data: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Read every file in a zip (stored or deflated entries). */
export async function unzip(buf: ArrayBuffer): Promise<Zip> {
  const v = new DataView(buf);
  // End of central directory: scan back over a possible comment.
  let eocd = -1;
  for (let i = buf.byteLength - 22; i >= Math.max(0, buf.byteLength - 22 - 65_535); i--) {
    if (u32(v, i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("Not a valid ePub (zip) file.");
  const count = u16(v, eocd + 10);
  if (count > MAX_ENTRIES) throw new Error("This ePub has too many files.");
  let at = u32(v, eocd + 16);
  const out: Zip = new Map();
  let total = 0;
  const decoder = new TextDecoder();
  for (let n = 0; n < count; n++) {
    if (u32(v, at) !== 0x02014b50) throw new Error("Corrupt ePub (central directory).");
    const method = u16(v, at + 10);
    const compressed = u32(v, at + 20);
    const size = u32(v, at + 24);
    const nameLen = u16(v, at + 28);
    const extraLen = u16(v, at + 30);
    const commentLen = u16(v, at + 32);
    const local = u32(v, at + 42);
    const name = decoder.decode(new Uint8Array(buf, at + 46, nameLen));
    at += 46 + nameLen + extraLen + commentLen;
    if (name.endsWith("/")) continue;
    total += size;
    if (total > MAX_TOTAL_BYTES) throw new Error("This ePub is too large to open here.");
    if (u32(v, local) !== 0x04034b50) throw new Error("Corrupt ePub (local header).");
    const start = local + 30 + u16(v, local + 26) + u16(v, local + 28);
    const data = new Uint8Array(buf, start, compressed);
    if (method === 0) out.set(name, data.slice());
    else if (method === 8) out.set(name, await inflateRaw(data));
    else throw new Error(`Unsupported compression in ePub (method ${method}).`);
  }
  return out;
}

function attrs(tag: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of tag.matchAll(/([\w:-]+)\s*=\s*("([^"]*)"|'([^']*)')/g)) out[m[1].toLowerCase()] = m[3] ?? m[4] ?? "";
  return out;
}

/** `href` relative to the file at `from` (both zip paths), percent-decoded, no fragment. */
export function resolvePath(from: string, href: string): string {
  const url = new URL(href, `http://zip/${from}`);
  return decodeURIComponent(url.pathname.slice(1));
}

export interface Chapter {
  /** Zip path of the XHTML file. */
  path: string;
}

export interface Book {
  title: string;
  chapters: Chapter[];
  zip: Zip;
  /** Manifest media types by zip path. */
  types: Map<string, string>;
}

const text = (zip: Zip, path: string) => {
  const data = zip.get(path);
  if (!data) throw new Error(`The ePub is missing ${path}.`);
  return new TextDecoder().decode(data);
};

/** Parse container.xml → OPF → manifest and spine. */
export function parseBook(zip: Zip): Book {
  const container = text(zip, "META-INF/container.xml");
  const rootfile = container.match(/<rootfile\b[^>]*>/i)?.[0];
  const opfPath = rootfile ? attrs(rootfile)["full-path"] : undefined;
  if (!opfPath) throw new Error("The ePub has no package document.");
  const opf = text(zip, opfPath);
  const manifest = new Map<string, { path: string; type: string }>();
  const types = new Map<string, string>();
  for (const m of opf.matchAll(/<item\b[^>]*>/gi)) {
    const a = attrs(m[0]);
    if (!a.id || !a.href) continue;
    const path = resolvePath(opfPath, a.href);
    manifest.set(a.id, { path, type: a["media-type"] ?? "" });
    types.set(path, a["media-type"] ?? "");
  }
  const chapters: Chapter[] = [];
  for (const m of opf.matchAll(/<itemref\b[^>]*>/gi)) {
    const a = attrs(m[0]);
    const item = a.idref ? manifest.get(a.idref) : undefined;
    if (item && a.linear !== "no" && zip.has(item.path)) chapters.push({ path: item.path });
  }
  if (chapters.length === 0) throw new Error("The ePub has no readable chapters.");
  const title = opf.match(/<dc:title\b[^>]*>([^<]*)</i)?.[1]?.trim() || "Untitled";
  return { title: decodeEntities(title), chapters, zip, types };
}

function decodeEntities(s: string): string {
  return s.replace(/&(amp|lt|gt|quot|apos|#(\d+));/g, (_, name: string, code?: string) =>
    code ? String.fromCharCode(Number(code)) : ({ amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" } as Record<string, string>)[name],
  );
}

/** Base styles so chapters are readable in both themes. */
const READER_CSS = `html{color-scheme:light dark}body{font:18px/1.6 Georgia,serif;max-width:40rem;margin:0 auto;padding:1.5rem 1rem 4rem}img,svg{max-width:100%;height:auto}`;

/**
 * A chapter as a standalone HTML document for `<iframe srcdoc>`: scripts and
 * inline event handlers removed, stylesheets inlined, and images pointed at
 * URLs from `urlFor` (blob: URLs in the browser).
 */
export function chapterDocument(book: Book, index: number, urlFor: (path: string) => string | undefined): string {
  const path = book.chapters[index].path;
  let html = text(book.zip, path);
  html = html.replace(/<\?xml[^>]*>/i, "").replace(/<!DOCTYPE[^>]*>/i, "");
  html = html.replace(/<script\b[\s\S]*?<\/script\s*>/gi, "").replace(/<script\b[^>]*\/>/gi, "");
  html = html.replace(/\son\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, "");
  // Inline linked stylesheets from the book.
  html = html.replace(/<link\b[^>]*>/gi, (tag) => {
    const a = attrs(tag);
    if (!/stylesheet/i.test(a.rel ?? "") || !a.href) return "";
    const css = book.zip.get(resolvePath(path, a.href));
    return css ? `<style>${new TextDecoder().decode(css).replace(/<\/style/gi, "")}</style>` : "";
  });
  // Images (HTML src, SVG href / xlink:href) → provided URLs; anything else is dropped.
  html = html.replace(/\s(src|href|xlink:href)\s*=\s*("([^"]*)"|'([^']*)')/gi, (whole, attr: string, _q, d?: string, s?: string) => {
    const value = d ?? s ?? "";
    if (/^(#|https?:|mailto:)/i.test(value)) return attr.toLowerCase() === "href" ? whole : "";
    const target = resolvePath(path, value);
    const type = book.types.get(target) ?? "";
    if (type.startsWith("image/")) {
      const url = urlFor(target);
      return url ? ` ${attr}="${url}"` : "";
    }
    // Links between chapters can't navigate inside the sandbox; keep them inert.
    return attr.toLowerCase() === "href" ? ` data-href="${value}"` : "";
  });
  return `<!doctype html><html><head><meta charset="utf-8"><style>${READER_CSS}</style></head><body>${bodyOf(html)}</body></html>`;
}

function bodyOf(html: string): string {
  const styles = [...html.matchAll(/<style\b[\s\S]*?<\/style\s*>/gi)].map((m) => m[0]).join("");
  const body = html.match(/<body\b[^>]*>([\s\S]*)<\/body\s*>/i)?.[1] ?? html;
  return styles + body;
}
