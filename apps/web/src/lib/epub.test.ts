import { describe, expect, it } from "vitest";
import { chapterDocument, parseBook, resolvePath, unzip } from "./epub";

/** A minimal zip writer (CRCs are left 0: the reader doesn't check them). */
async function zip(files: Record<string, string>, deflate: string[] = []): Promise<ArrayBuffer> {
  const enc = new TextEncoder();
  const locals: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  for (const [name, content] of Object.entries(files)) {
    const raw = enc.encode(content);
    const method = deflate.includes(name) ? 8 : 0;
    const data =
      method === 8
        ? new Uint8Array(await new Response(new Blob([raw]).stream().pipeThrough(new CompressionStream("deflate-raw"))).arrayBuffer())
        : raw;
    const n = enc.encode(name);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(8, method, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, raw.length, true);
    local.setUint16(26, n.length, true);
    locals.push(new Uint8Array(local.buffer), n, data);
    const c = new DataView(new ArrayBuffer(46));
    c.setUint32(0, 0x02014b50, true);
    c.setUint16(10, method, true);
    c.setUint32(20, data.length, true);
    c.setUint32(24, raw.length, true);
    c.setUint16(28, n.length, true);
    c.setUint32(42, offset, true);
    central.push(new Uint8Array(c.buffer), n);
    offset += 30 + n.length + data.length;
  }
  const centralSize = central.reduce((s, b) => s + b.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, Object.keys(files).length, true);
  end.setUint16(10, Object.keys(files).length, true);
  end.setUint32(12, centralSize, true);
  end.setUint32(16, offset, true);
  return new Blob([...locals, ...central, new Uint8Array(end.buffer)] as BlobPart[]).arrayBuffer();
}

const book = (chapter2 = "<p>Second</p>") => ({
  "mimetype": "application/epub+zip",
  "META-INF/container.xml": `<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>`,
  "OEBPS/content.opf": `<package><metadata><dc:title>Dune &amp; Co</dc:title></metadata><manifest>
      <item id="c1" href="text/one.xhtml" media-type="application/xhtml+xml"/>
      <item media-type="application/xhtml+xml" href="text/two%20b.xhtml" id="c2"/>
      <item id="css" href="style.css" media-type="text/css"/>
      <item id="img" href="images/cover.png" media-type="image/png"/>
    </manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>`,
  "OEBPS/text/one.xhtml": `<?xml version="1.0"?><!DOCTYPE html><html><head><link rel="stylesheet" href="../style.css"/></head><body onload="evil()"><h1>One</h1><img src="../images/cover.png"/><script>alert(1)</script><a href="two%20b.xhtml">next</a></body></html>`,
  "OEBPS/text/two b.xhtml": `<html><body>${chapter2}</body></html>`,
  "OEBPS/style.css": "h1 { color: teal }",
  "OEBPS/images/cover.png": "PNG",
});

describe("ePub reading", () => {
  it("unzips stored and deflated entries", async () => {
    const z = await unzip(await zip({ "a.txt": "plain", "b.txt": "squeezed ".repeat(50) }, ["b.txt"]));
    expect(new TextDecoder().decode(z.get("a.txt"))).toBe("plain");
    expect(new TextDecoder().decode(z.get("b.txt"))).toBe("squeezed ".repeat(50));
    await expect(unzip(new ArrayBuffer(10))).rejects.toThrow("Not a valid ePub");
  });

  it("follows container → OPF → spine", async () => {
    const b = parseBook(await unzip(await zip(book(), ["OEBPS/text/one.xhtml"])));
    expect(b.title).toBe("Dune & Co");
    expect(b.chapters.map((c) => c.path)).toEqual(["OEBPS/text/one.xhtml", "OEBPS/text/two b.xhtml"]);
    expect(resolvePath("OEBPS/text/one.xhtml", "../images/cover.png")).toBe("OEBPS/images/cover.png");
  });

  it("renders chapters without scripts, with styles inlined and images mapped", async () => {
    const b = parseBook(await unzip(await zip(book())));
    const doc = chapterDocument(b, 0, (p) => (p === "OEBPS/images/cover.png" ? "blob:cover" : undefined));
    expect(doc).toContain("<h1>One</h1>");
    expect(doc).toContain("h1 { color: teal }");
    expect(doc).toContain('src="blob:cover"');
    expect(doc).not.toMatch(/<script|alert\(1\)|onload/i);
    expect(doc).toContain('data-href="two%20b.xhtml"');
    expect(chapterDocument(b, 1, () => undefined)).toContain("<p>Second</p>");
  });

  it("rejects ePubs without a readable spine", async () => {
    const files = { ...book(), "OEBPS/content.opf": "<package><manifest/><spine/></package>" };
    expect(() => parseBook(new Map(Object.entries(files).map(([k, v]) => [k, new TextEncoder().encode(v)])))).toThrow("no readable chapters");
  });
});
