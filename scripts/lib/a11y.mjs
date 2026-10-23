// Accessibility checks over the browser's computed accessibility tree (CDP).
// Flags interactive controls without an accessible name, images without alt
// text, and pages without a heading. Not a full audit; it catches the
// regressions that matter most for screen-reader and voice-control users.

const INTERACTIVE = new Set(["button", "link", "textbox", "searchbox", "combobox", "checkbox", "radio", "slider", "spinbutton", "switch", "tab", "menuitem", "listbox"]);

/** Problems on the current page, as human-readable strings. */
export async function audit(page) {
  await page.send("Accessibility.enable", {});
  const { nodes } = await page.send("Accessibility.getFullAXTree", {});
  const problems = [];
  let headings = 0;
  for (const n of nodes) {
    if (n.ignored) continue;
    const role = n.role?.value;
    const name = (n.name?.value ?? "").trim();
    if (role === "heading") headings++;
    if (INTERACTIVE.has(role) && !name) {
      const desc = await describe(page, n.backendDOMNodeId);
      problems.push(`${role} without an accessible name: ${desc}`);
    }
    if (role === "image" && !name) problems.push(`image without alt text: ${await describe(page, n.backendDOMNodeId)}`);
  }
  if (headings === 0) problems.push("no heading on the page");
  return problems;
}

async function describe(page, backendNodeId) {
  if (!backendNodeId) return "(unknown element)";
  try {
    const { outerHTML } = await page.send("DOM.getOuterHTML", { backendNodeId });
    return outerHTML.replace(/\s+/g, " ").slice(0, 160);
  } catch {
    return "(detached element)";
  }
}
