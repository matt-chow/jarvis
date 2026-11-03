// Design tour (D-050): screenshots of the main screens at phone, tablet, and
// desktop widths in Light and Night, on an isolated wrangler dev with a little
// sample data created through the API. For design review only; nothing is
// asserted. Run after `npm run build`:
//
//   SCREENSHOT_DIR=/path/to/out node scripts/design-tour.mjs [route …]
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { launch, waitFor } from "./lib/cdp.mjs";
import { startDevServer } from "./lib/devserver.mjs";

const OUT = process.env.SCREENSHOT_DIR;
if (!OUT) throw new Error("set SCREENSHOT_DIR");
mkdirSync(OUT, { recursive: true });
const routes = process.argv.slice(2).length ? process.argv.slice(2) : ["home", "tasks", "calendar", "habits", "budget", "music", "reminders", "notes", "journal", "projects", "settings", "modules"];
const SIZES = { phone: { width: 390, height: 844, mobile: true }, tablet: { width: 820, height: 1100, mobile: false }, desktop: { width: 1440, height: 900, mobile: false } };
const THEMES = (process.env.THEMES ?? "night,light").split(",");

const server = await startDevServer({ port: 8793, name: "tour" });
const { page, close } = await launch();
const pad = (n) => String(n).padStart(2, "0");
const day = (offset) => {
  const d = new Date(Date.now() + offset * 86_400_000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
const post = (path, body, method = "POST") =>
  page.evaluate(
    `fetch(${JSON.stringify(path)}, { method: ${JSON.stringify(method)}, headers: { "Content-Type": "application/json", Origin: location.origin }, body: ${JSON.stringify(JSON.stringify(body))} }).then(async (r) => r.ok ? (r.status === 204 ? null : r.json()) : Promise.reject(new Error(r.status + " " + await r.text())))`,
  );

try {
  await page.addVirtualAuthenticator();
  await page.navigate(server.origin);
  await page.waitForText("first-run setup");
  await page.fill("your name", "Sam Rivera");
  await page.fill("setup token", server.setupToken);
  await page.click("create passkey");
  await waitFor("signed in", async () => /Good (morning|afternoon|evening)/.test(await page.text()));

  // Sample data: a believable day.
  const today = day(0);
  for (const [title, due, priority] of [
    ["Send the quarterly invoice", day(-1), "high"],
    ["Book the dentist", today, "medium"],
    ["Draft talk outline", today, "none"],
    ["Renew library card", day(3), "low"],
  ])
    await post("/api/v1/tasks", { title, due_date: due, priority }).catch(() => {});
  await post("/api/v1/events", { title: "Design review", start_date: today, start_time: "10:00", end_time: "11:00" }).catch((e) => console.log(e.message));
  await post("/api/v1/events", { title: "Lunch with Ana", start_date: today, start_time: "13:30", end_time: "14:30" }).catch((e) => console.log(e.message));
  await post("/api/v1/habits", { name: "Read 20 pages", days: [0, 1, 2, 3, 4, 5, 6] }).catch(() => {});
  await post("/api/v1/habits", { name: "Stretch", days: [1, 3, 5] }).catch(() => {});
  await post("/api/v1/reminders", { title: "Water the plants", due_date: today, due_time: "18:00" }).catch(() => {});
  await post("/api/v1/reminders", { title: "Call Mum" }).catch(() => {});
  if (process.env.TOUR_SEED) await page.evaluate(process.env.TOUR_SEED).catch((e) => console.log("seed:", e.message));

  for (const theme of THEMES) {
    await page.evaluate(`localStorage.setItem("theme", ${JSON.stringify(theme)}); document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)})`);
    for (const [size, m] of Object.entries(SIZES)) {
      await page.send("Emulation.setDeviceMetricsOverride", { width: m.width, height: m.height, deviceScaleFactor: 1, mobile: m.mobile });
      for (const route of routes) {
        await page.hash(`/${route}`);
        await new Promise((r) => setTimeout(r, 900));
        await page.screenshot(join(OUT, `${route}-${size}-${theme}.png`), { width: m.width, height: m.height });
      }
    }
  }
  await page.send("Emulation.clearDeviceMetricsOverride", {});
  console.log(`screenshots in ${OUT}`);
} finally {
  await close();
  server.stop();
}
