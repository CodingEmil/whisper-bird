import { expect, test, type Page } from "@playwright/test";

/** Overlay mit nachgebauter Tauri-Brücke, über die der Test Events schickt. */
async function openOverlay(page: Page) {
  await page.addInitScript(() => {
    const w = window as any;
    let nextId = 0;
    const callbacks: Record<number, (e: unknown) => void> = {};
    const listeners: Record<string, number[]> = {};
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      transformCallback: (cb: (e: unknown) => void) => {
        callbacks[++nextId] = cb;
        return nextId;
      },
      invoke: async (command: string, args: any) => {
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
  await page.goto("/overlay.html");
  await expect.poll(() => page.evaluate(() => (window as any).hasListener("meeting-update"))).toBe(true);
}

const emit = (page: Page, event: string, payload: unknown) =>
  page.evaluate(([event, payload]) => (window as any).emit(event, payload), [event, payload] as const);

test("Läuft ein Meeting, leuchtet um die Bubble ein Ring mit dem Pegel", async ({ page }) => {
  await openOverlay(page);
  const bubble = page.locator(".bubble");
  await expect(bubble).toBeVisible();
  await expect(bubble).not.toHaveClass(/meeting/);

  // Der lautere der beiden Kanäle zählt.
  await emit(page, "meeting-level", { id: "m1", mic: 0.2, system: 0.7, duration_ms: 100 });
  await expect(bubble).toHaveClass(/meeting/);
  await expect.poll(() => bubble.evaluate((el) => getComputedStyle(el).getPropertyValue("--level"))).toBe("0.7");

  // Ein anderes Meeting wird fertig: Der Ring bleibt.
  await emit(page, "meeting-update", { id: "alt", status: "done" });
  await expect(bubble).toHaveClass(/meeting/);

  // Das laufende Meeting stoppt: Der Ring geht aus.
  await emit(page, "meeting-update", { id: "m1", status: "processing" });
  await expect(bubble).not.toHaveClass(/meeting/);
});
