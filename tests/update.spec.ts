import { expect, test, type Page } from "@playwright/test";

async function openApp(page: Page, update: Record<string, unknown>) {
  await page.addInitScript((update) => {
    const w = window as any;
    let nextId = 0;
    w.calls = [];
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: () => ++nextId,
      invoke: async (command: string) => {
        w.calls.push(command);
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "get_settings") {
          return { ui_language: "de", onboarding_done: true, shortcut_dictate: "Ctrl+Shift+Space", enhance_mode: "off" };
        }
        if (command === "list_models" || command === "history_query") return [];
        if (command === "app_info") return { asr_loaded: null, devices: [] };
        if (command === "update_status" || command === "update_check") return update;
        return null;
      },
    };
  }, update);
  await page.goto("/");
}

const calls = (page: Page) => page.evaluate(() => (window as any).calls as string[]);

const status = {
  current: "0.1.0-beta.4",
  latest: null,
  url: null,
  installable: false,
  ready: false,
  installing: false,
  checking: false,
  checked_at: Date.now(),
  error: null,
  just_updated: false,
  notes: [],
};

test("Seitenleiste zeigt die Version und sucht auf Knopfdruck erneut", async ({ page }) => {
  await openApp(page, status);
  const nav = page.getByRole("navigation");
  await expect(nav).toContainText("v0.1.0-beta.4 · aktuell");

  await nav.getByRole("button", { name: "Nach Updates suchen" }).click();
  await expect.poll(() => calls(page)).toContain("update_check");
});

test("Ein Update führt von der Seitenleiste zur Installation", async ({ page }) => {
  await openApp(page, { ...status, latest: "0.1.0-beta.5", url: "https://example.com", installable: true });
  await page.getByRole("navigation").getByRole("button", { name: "Update auf 0.1.0-beta.5" }).click();

  await expect(page.getByRole("heading", { name: "Hilfe & Info" })).toBeVisible();
  await expect(page.getByText("Version 0.1.0-beta.5 ist verfügbar.")).toBeVisible();
  await page.getByRole("button", { name: "Installieren" }).click();
  await expect.poll(() => calls(page)).toContain("update_install");
});

test("Ohne Selbst-Update führt Herunterladen zur Release-Seite", async ({ page }) => {
  await openApp(page, { ...status, latest: "0.1.0-beta.5", url: "https://example.com" });
  await page.getByRole("navigation").getByRole("button", { name: "Update auf 0.1.0-beta.5" }).click();

  await expect(page.getByRole("button", { name: "Installieren" })).toHaveCount(0);
  await page.getByRole("button", { name: "Herunterladen" }).click();
  await expect.poll(() => calls(page)).toContain("update_open_page");
});

test("Während der Installation ist der Knopf gesperrt", async ({ page }) => {
  await openApp(page, { ...status, latest: "0.1.0-beta.5", url: "https://example.com", installable: true, ready: true, installing: true });
  await page.getByRole("navigation").getByRole("button", { name: "Update auf 0.1.0-beta.5" }).click();

  await expect(page.getByText("Wird installiert, Whisper Bird startet gleich neu …")).toBeVisible();
  await expect(page.getByRole("button", { name: "Installieren" })).toBeDisabled();
});

test("Nach einem Update erscheint einmal ein Hinweis mit den Neuerungen", async ({ page }) => {
  await openApp(page, { ...status, current: "0.1.0-beta.5", just_updated: true, notes: ["Update-Anzeige", "Hinweis nach dem Update"] });
  const dialog = page.getByRole("dialog", { name: "Auf Version 0.1.0-beta.5 aktualisiert" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("Das ist neu:");
  await expect(dialog.getByRole("listitem")).toHaveText(["Update-Anzeige", "Hinweis nach dem Update"]);

  await dialog.getByRole("button", { name: "Alles klar" }).click();
  await expect(dialog).toBeHidden();
  await expect.poll(() => calls(page)).toContain("update_notice_seen");
});

test("Ohne Neuerungen bleibt es bei einem kurzen Hinweis, Esc schließt ihn", async ({ page }) => {
  await openApp(page, { ...status, current: "0.1.0-beta.5", just_updated: true });
  const dialog = page.getByRole("dialog", { name: "Auf Version 0.1.0-beta.5 aktualisiert" });
  await expect(dialog).toContainText("Whisper Bird ist wieder auf dem neuesten Stand.");
  await expect(dialog.getByRole("list")).toHaveCount(0);

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect.poll(() => calls(page)).toContain("update_notice_seen");
});
