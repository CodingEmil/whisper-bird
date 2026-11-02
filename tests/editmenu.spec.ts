import { expect, test, type Page } from "@playwright/test";

/** Bearbeiten-Menü mit nachgebauter Tauri-Brücke, über die der Test Events schickt. */
async function openMenu(page: Page) {
  await page.addInitScript(() => {
    const w = window as any;
    let nextId = 0;
    const callbacks: Record<number, (e: unknown) => void> = {};
    const listeners: Record<string, number[]> = {};
    w.calls = [];
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: (cb: (e: unknown) => void) => {
        callbacks[++nextId] = cb;
        return nextId;
      },
      invoke: async (command: string, args: any) => {
        w.calls.push(command);
        if (command === "plugin:event|listen") {
          (listeners[args.event] ??= []).push(args.handler);
          return args.handler;
        }
        if (command === "get_settings") return { ui_language: "de" };
        return null;
      },
    };
    w.hasListener = (event: string) => !!listeners[event];
    w.emit = (event: string, payload: unknown) => (listeners[event] ?? []).forEach((id) => callbacks[id]({ event, id, payload }));
  });
  await page.goto("/editmenu.html");
  await expect.poll(() => page.evaluate(() => (window as any).hasListener("editmenu-selection"))).toBe(true);
}

const emit = (page: Page, event: string, payload: unknown) =>
  page.evaluate(([event, payload]) => (window as any).emit(event, payload), [event, payload] as const);

test("Neue Markierung bei offenem Menü ersetzt den Text, die Anweisung bleibt", async ({ page }) => {
  await openMenu(page);
  await emit(page, "editmenu-open", { selection: "Erster Text", app: "Editor", actions: [], translate_target: "Englisch" });
  await expect(page.getByText("Erster Text")).toBeVisible();
  const input = page.getByRole("textbox", { name: "Beschreibe eine Änderung oder sag sie …" });
  await input.fill("kürzer");

  await emit(page, "editmenu-selection", { selection: "Zweiter Text aus Word", app: "Word" });
  await expect(page.getByText("Zweiter Text aus Word")).toBeVisible();
  await expect(page.getByText("Erster Text")).toHaveCount(0);
  await expect(page.getByText("Word", { exact: true })).toBeVisible();
  await expect(input).toHaveValue("kürzer");
});

test("Das Menü erscheint sofort, die Markierung kommt nach", async ({ page }) => {
  await openMenu(page);
  await emit(page, "editmenu-open", { selection: null, pending: true, app: "Editor", actions: [], translate_target: "" });
  await expect(page.locator(".em-selection[aria-busy=true]")).toBeVisible();
  await expect(page.getByText("Kein Text markiert")).toHaveCount(0);

  await emit(page, "editmenu-selection", { selection: "Markierter Text", app: "Editor" });
  await expect(page.getByText("Markierter Text")).toBeVisible();
  await expect(page.locator(".em-selection[aria-busy=true]")).toHaveCount(0);
});

test("Das Menü meldet seine Tasten, damit das Kürzel es auch mit Fokus schließt", async ({ page }) => {
  await openMenu(page);
  await emit(page, "editmenu-open", { selection: null, app: "Editor", actions: [], translate_target: "" });
  await page.getByRole("textbox").press("Control+Shift+E");
  await expect.poll(() => page.evaluate(() => (window as any).calls as string[])).toContain("window_key");

  // Schloss das Kürzel das Menü vor dem Loslassen, hält Chromium E für noch gedrückt
  // und meldet den nächsten Druck als Wiederholung. Auch der muss ankommen.
  await page.evaluate(() => {
    (window as any).calls = [];
    window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyE", key: "E", ctrlKey: true, shiftKey: true, repeat: true }));
  });
  await expect.poll(() => page.evaluate(() => (window as any).calls as string[])).toContain("window_key");
});
