import { expect, test } from "@playwright/test";

const asr = (id: string, name: string, installed = true) => ({
  id, name, description_de: "", description_en: "", kind: "asr", family: "whisper",
  size: 1, languages: [], speed: 3, accuracy: 3, recommended: false, installed, downloading: false,
});

test("Meetings und Diktat bekommen je ein eigenes Modell", async ({ page }) => {
  await page.route("**/src/App.tsx", (route) => route.fulfill({
    contentType: "application/javascript",
    body: 'export { default } from "/src/pages/Models.tsx";',
  }));
  await page.addInitScript((models) => {
    const w = window as any;
    let nextId = 0;
    let settings: any = { ui_language: "de", asr_model: "fast", meeting_asr_model: "", accelerator: "auto" };
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: () => ++nextId,
      invoke: async (command: string, args: any) => {
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "get_settings") return settings;
        if (command === "app_info") return { devices: [] };
        if (command === "list_models") return models;
        if (command === "set_settings") {
          w.patch = args.patch;
          settings = { ...settings, ...args.patch };
          return { settings, shortcut_errors: [] };
        }
      },
    };
  }, [asr("fast", "Schnell"), asr("exact", "Genau"), asr("missing", "Fehlt", false)]);
  await page.goto("/");

  const meeting = page.getByRole("combobox", { name: "Meetings und Dateien" });
  await expect(meeting).toHaveValue("");
  // Nicht installierte Modelle stehen nicht zur Wahl.
  await expect(meeting.getByRole("option")).toHaveText(["Wie beim Diktat", "Schnell", "Genau"]);

  await meeting.selectOption("exact");
  await expect.poll(() => page.evaluate(() => (window as any).patch)).toEqual({ meeting_asr_model: "exact" });
  await expect(page.getByRole("combobox", { name: "Diktat" })).toHaveValue("fast");
});

test("Remote-Spracherkennung wird gespeichert, eingeklappt und ist wählbar", async ({ page }) => {
  await page.route("**/src/App.tsx", (route) => route.fulfill({
    contentType: "application/javascript",
    body: 'export { default } from "/src/pages/Models.tsx";',
  }));
  await page.addInitScript((models) => {
    const w = window as any;
    let nextId = 0;
    let settings: any = {
      ui_language: "de", asr_model: "fast", meeting_asr_model: "", accelerator: "auto",
      asr_remote_url: "", asr_remote_key: "", asr_remote_model: "whisper-1",
    };
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: () => ++nextId,
      invoke: async (command: string, args: any) => {
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "get_settings") return settings;
        if (command === "app_info") return { devices: [] };
        if (command === "list_models") return models;
        if (command === "set_settings") {
          w.patch = args.patch;
          settings = { ...settings, ...args.patch };
          return { settings, shortcut_errors: [] };
        }
      },
    };
  }, [asr("fast", "Schnell")]);
  await page.goto("/");

  const save = page.getByRole("button", { name: "Speichern" });
  await expect(save).toBeDisabled();
  await page.getByRole("textbox", { name: "Basis-URL" }).fill(" https://api.example.com/v1 ");
  await page.getByLabel("API-Schlüssel").fill("geheim");
  await save.click();

  await expect.poll(() => page.evaluate(() => (window as any).patch)).toEqual({
    asr_remote_url: "https://api.example.com/v1", asr_remote_key: "geheim", asr_remote_model: "whisper-1",
  });
  // Eingeklappt: keine Felder mehr, nur die hinterlegte Adresse.
  await expect(page.getByText("Hinterlegt")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Basis-URL" })).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "Meetings und Dateien" }).getByRole("option"))
    .toHaveText(["Wie beim Diktat", "Schnell", "Remote (whisper-1)"]);
});
