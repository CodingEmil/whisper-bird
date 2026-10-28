import { expect, test } from "@playwright/test";

// Hat die WebView den Fokus, sieht der globale Hook nichts; das Fenster meldet seine Tasten selbst.
test("meldet Tasten im Testfeld ans Backend", async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as any;
    let nextId = 0;
    w.keys = [];
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: () => ++nextId,
      invoke: async (command: string, args: any) => {
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "get_settings") {
          return { ui_language: "de", onboarding_done: true, shortcut_dictate: "Ctrl+Shift+Space", enhance_mode: "off" };
        }
        if (command === "list_models" || command === "history_query") return [];
        if (command === "app_info") return { asr_loaded: null };
        if (command === "window_key") w.keys.push(`${args.down ? "↓" : "↑"} ${args.modifiers} ${args.key ?? "-"}`);
        if (command === "window_blur") w.keys.push("blur");
        return null;
      },
    };
  });
  await page.goto("/");
  await page.getByLabel("Ausprobieren").click();
  await page.evaluate(() => { (window as any).keys = []; }); // StrictMode baut den Effekt zweimal auf

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.down("Space");
  await page.keyboard.down("Space"); // Wiederholung
  await page.keyboard.up("Space");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));

  await expect.poll(() => page.evaluate(() => (window as any).keys)).toEqual([
    "↓ CtrlLeft -",
    "↓ CtrlLeft+ShiftLeft -",
    "↓ CtrlLeft+ShiftLeft Space",
    "↑ CtrlLeft+ShiftLeft Space",
    "↑ CtrlLeft -",
    "↑  -",
    "blur",
  ]);
  await expect(page.getByLabel("Ausprobieren")).toHaveValue("");
});
