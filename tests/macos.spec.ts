import { expect, test, type Page } from "@playwright/test";

// Die Oberfläche unter macOS: Das Backend setzt window.__WB_PLATFORM__ vor dem Laden
// (lib.rs: platform_script). Hier wird nur die native Schnittstelle nachgebildet.
async function open(page: Page, entry: string, extra: Record<string, unknown> = {}) {
  await page.route("**/src/App.tsx", (route) => route.fulfill({
    contentType: "application/javascript",
    body: `export { default } from "${entry}";`,
  }));
  await page.addInitScript((extra) => {
    const w = window as any;
    w.__WB_PLATFORM__ = "macos";
    let nextId = 0;
    const settings = {
      ui_language: "de",
      onboarding_done: false,
      asr_model: "",
      shortcut_dictate: "CtrlLeft+Space",
      shortcut_dictate_enhance: "",
      shortcut_edit: "Ctrl+Shift+E",
      shortcut_meeting: "",
      enhance_mode: "off",
      ...extra,
    };
    w.calls = [];
    w.perms = { required: true, accessibility: false, microphone: "not_determined" };
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: () => ++nextId,
      invoke: async (command: string, args: any) => {
        w.calls.push({ command, ...args });
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "get_settings") return { ...settings };
        if (command === "set_settings") {
          Object.assign(settings, args.patch);
          return { settings: { ...settings }, shortcut_errors: [] };
        }
        if (command === "list_models") return [];
        if (command === "permissions_status") return { ...w.perms };
        return undefined;
      },
    };
  }, extra);
  await page.goto("/");
}

test("Shortcuts zeigen Mac-Symbole statt Strg", async ({ page }) => {
  await open(page, "/src/pages/Shortcuts.tsx");
  await expect(page.locator(".shortcut-input").first()).toHaveText("⌃ links + Leertaste");
  await expect(page.locator(".shortcut-input").nth(2)).toHaveText("⌃ + ⇧ + E");
});

test("Einrichtung beginnt mit den Freigaben und fragt nach Bedienungshilfen", async ({ page }) => {
  await open(page, "/src/pages/Onboarding.tsx");
  await expect(page.getByRole("heading", { name: "Zwei Freigaben für macOS" })).toBeVisible();
  const grant = page.getByRole("button", { name: "Freigeben" }).first();
  await grant.click();
  await expect.poll(() => page.evaluate(() => (window as any).calls.find((c: any) => c.command === "permission_request")))
    .toEqual({ command: "permission_request", kind: "accessibility" });

  // Danach führt der Knopf direkt in die Einstellungen, darunter steht die Anleitung.
  await expect(page.getByRole("heading", { name: "So geht's in den Systemeinstellungen" })).toBeVisible();
  await page.getByRole("button", { name: "Einstellungen öffnen" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).calls.find((c: any) => c.command === "permission_open_settings")))
    .toEqual({ command: "permission_open_settings", kind: "accessibility" });

  // Freigabe in den Systemeinstellungen: Die Abfrage alle 2 s bemerkt sie.
  await page.evaluate(() => { (window as any).perms.accessibility = true; });
  await expect(page.getByText("Freigegeben")).toBeVisible({ timeout: 5000 });
  await expect(page.getByRole("heading", { name: "So geht's in den Systemeinstellungen" })).toBeHidden();

  await page.getByRole("button", { name: "Weiter" }).click();
  await expect(page.getByRole("heading", { name: "Modell wählen" })).toBeVisible();
});
