// Screenshot a route at both reference viewports, light and dark.
// Usage: node scripts/ui-shot.mjs <route> <outdir> [base-url]
// e.g.   node scripts/ui-shot.mjs '/doc/herd-movement-phase-lock' shots
import { chromium } from "playwright-core";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";

const [route = "/", outDir = "shots", base = "http://localhost:5173"] = process.argv.slice(2);
const executablePath = join(
  homedir(),
  ".cache/ms-playwright/chromium_headless_shell-1228/chrome-headless-shell-linux64/chrome-headless-shell",
);

const VIEWPORTS = [
  { name: "desktop", width: 1280, height: 800 },
  { name: "phone", width: 390, height: 844 },
];
const SCHEMES = ["light", "dark"];

mkdirSync(outDir, { recursive: true });
const browser = await chromium.launch({ executablePath });
const slug = route.replaceAll("/", "-").replace(/^-|-$/g, "") || "root";

for (const viewport of VIEWPORTS) {
  for (const colorScheme of SCHEMES) {
    const context = await browser.newContext({ viewport, colorScheme });
    await context.addInitScript(() => {
      localStorage.setItem("bc.settings.storeToken", "dev");
    });
    const page = await context.newPage();
    await page.goto(`${base}/#${route}`, { waitUntil: "load" });
    await page.waitForSelector("main :not(:empty)", { timeout: 15000 });
    await page.waitForTimeout(500);
    const file = join(outDir, `${slug}.${viewport.name}.${colorScheme}.png`);
    await page.screenshot({ path: file, fullPage: true });
    console.log(file);
    await context.close();
  }
}
await browser.close();
