import { expect, test, type Page } from "@playwright/test";

async function openModels(page: Page) {
  await page.route("**/src/App.tsx", (route) => route.fulfill({
    contentType: "application/javascript",
    body: 'export { default } from "/src/pages/Models.tsx";',
  }));
  await page.addInitScript(() => {
    const w = window as any;
    let nextId = 0;
    w.calls = [];
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: () => ++nextId,
      invoke: async (command: string, args: any) => {
        w.calls.push(command);
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "get_settings") return { ui_language: "de", asr_model: "base", accelerator: "auto" };
        if (command === "app_info") return { devices: [] };
        if (command === "list_models") {
          return [{
            id: "base", name: "Whisper Base", description_de: "", description_en: "", kind: "asr", family: "whisper",
            size: 1, languages: [], speed: 3, accuracy: 3, recommended: false, installed: true, downloading: false,
          }];
        }
        if (command === "delete_model") w.deleted = args.id;
      },
    };
    // Wie das Dialog-Plugin von Tauri: ein async `confirm`, dessen Promise immer wahr ist.
    w.confirm = async () => {
      w.calls.push("window.confirm");
      return false;
    };
  });
  await page.goto("/");
}

const calls = (page: Page) => page.evaluate(() => (window as any).calls as string[]);

test("Löschen wartet auf die Bestätigung im eigenen Dialog", async ({ page }) => {
  await openModels(page);
  const dialog = page.getByRole("dialog", { name: "Modell löschen?" });

  await page.getByRole("button", { name: "Löschen" }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("„Whisper Base“ wird von diesem PC entfernt.");
  await expect(dialog.getByRole("button", { name: "Abbrechen" })).toBeFocused();
  expect(await calls(page)).not.toContain("delete_model");

  await dialog.getByRole("button", { name: "Abbrechen" }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: "Löschen" }).click();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: "Löschen" }).click();
  await page.mouse.click(5, 5); // Hintergrund
  await expect(dialog).toBeHidden();
  expect(await calls(page)).not.toContain("delete_model");

  await page.getByRole("button", { name: "Löschen" }).click();
  await dialog.getByRole("button", { name: "Löschen" }).click();
  await expect(dialog).toBeHidden();
  await expect.poll(() => page.evaluate(() => (window as any).deleted)).toBe("base");
  expect(await calls(page)).not.toContain("window.confirm");
});
