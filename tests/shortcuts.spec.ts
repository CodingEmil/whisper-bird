import { expect, test, type Page } from "@playwright/test";

async function openShortcuts(page: Page) {
  // Only native IPC is mocked. Keyboard events go through the actual recorder;
  // no synthetic "shortcut-capture" results are injected by these tests.
  await page.route("**/src/App.tsx", (route) => route.fulfill({
    contentType: "application/javascript",
    body: 'export { default } from "/src/pages/Shortcuts.tsx";',
  }));
  await page.addInitScript(() => {
    const w = window as any;
    let nextId = 0;
    const listeners = new Map<number, unknown>();
    const settings = {
      ui_language: "de",
      shortcut_dictate: "CtrlLeft+Space",
      shortcut_dictate_enhance: "",
      shortcut_edit: "Ctrl+Shift+E",
      shortcut_meeting: "",
      enhance_mode: "off",
    };
    w.captureCalls = [];
    w.savedPatches = [];
    w.settings = settings;
    w.activeSession = null;
    w.failCapture = false;
    w.captureDelay = 0;
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: (_event: string, id: number) => listeners.delete(id),
    };
    w.__TAURI_INTERNALS__ = {
      transformCallback: (callback: unknown) => {
        const id = ++nextId;
        listeners.set(id, callback);
        return id;
      },
      invoke: async (command: string, args: any) => {
        if (command === "plugin:event|listen") return ++nextId;
        if (command === "plugin:event|unlisten") return;
        if (command === "get_settings") return { ...settings };
        if (command === "set_settings") {
          w.savedPatches.push(args.patch);
          Object.assign(settings, args.patch);
          return { settings: { ...settings }, shortcut_errors: [] };
        }
        if (command.endsWith("shortcut_capture")) {
          w.captureCalls.push({ command, ...args });
          if (command === "start_shortcut_capture") {
            if (w.captureDelay) await new Promise((resolve) => setTimeout(resolve, w.captureDelay));
            if (w.failCapture) throw new Error("Pause fehlgeschlagen");
            w.activeSession = args.session;
          } else if (w.activeSession === args.session) {
            w.activeSession = null;
          }
        }
      },
    };
  });
  await page.goto("/");
}

for (const [combination, expected] of [
  ["Control+Space", "CtrlLeft+Space"],
  ["Control+Shift+Space", "CtrlLeft+ShiftLeft+Space"],
  ["Control+Alt+Enter", "CtrlLeft+OptLeft+Return"],
  ["a", "A"],
  ["F8", "F8"],
  ["Enter", "Return"],
  ["Space", "Space"],
  ["ControlRight", "CtrlRight"],
  ["ShiftRight", "ShiftRight"],
  ["Numpad1", "Keypad1"],
]) {
  test(`records ${combination} from real keyboard events without a native listener`, async ({ page }) => {
    await openShortcuts(page);
    const input = page.locator(".shortcut-input").first();
    await input.click();
    await expect(input).toHaveClass(/recording/);
    await page.keyboard.press(combination);
    await expect(input).not.toHaveClass(/recording/);
    await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
      shortcut_dictate: expected,
    }]);
    expect(await page.evaluate(() => (window as any).activeSession)).toBeNull();
  });
}

for (const releaseCtrlFirst of [false, true]) {
  test(`waits for release of the full combination, Ctrl released ${releaseCtrlFirst ? "first" : "last"}`, async ({ page }) => {
    await openShortcuts(page);
    const input = page.locator(".shortcut-input").first();
    await input.click();
    await page.keyboard.down("Control");
    await page.keyboard.down("Shift");
    await page.keyboard.down("Space");
    await expect(input).toHaveText("Strg links + Umschalt links + Leertaste");
    await expect(input).toHaveClass(/recording/);
    await page.keyboard.up(releaseCtrlFirst ? "Control" : "Space");
    await page.keyboard.up("Shift");
    await expect(input).toHaveClass(/recording/);
    await page.keyboard.up(releaseCtrlFirst ? "Space" : "Control");
    await expect(input).not.toHaveClass(/recording/);
    await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
      shortcut_dictate: "CtrlLeft+ShiftLeft+Space",
    }]);
  });
}

// Pressed "at the same time", the key often arrives a few ms before the last modifier.
for (const [modifier, expected] of [["Shift", "CtrlLeft+ShiftLeft+Space"], ["Alt", "CtrlLeft+OptLeft+Space"]]) {
  test(`${modifier} pressed after Space still belongs to the combination`, async ({ page }) => {
    await openShortcuts(page);
    const input = page.locator(".shortcut-input").first();
    await input.click();
    await page.keyboard.down("Control");
    await page.keyboard.down("Space");
    await page.keyboard.down(modifier);
    await page.keyboard.up("Space");
    await page.keyboard.up(modifier);
    await page.keyboard.up("Control");
    await expect(input).not.toHaveClass(/recording/);
    await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
      shortcut_dictate: expected,
    }]);
  });
}

test("the combination is fixed once the first key is released", async ({ page }) => {
  await openShortcuts(page);
  const input = page.locator(".shortcut-input").first();
  await input.click();
  await page.keyboard.down("Control");
  await page.keyboard.down("Space");
  await page.keyboard.up("Space");
  await page.keyboard.down("Shift");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await expect(input).not.toHaveClass(/recording/);
  await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
    shortcut_dictate: "CtrlLeft+Space",
  }]);
});

test("setting Enhance shortcut enables the second-shortcut mode", async ({ page }) => {
  await openShortcuts(page);
  await page.locator(".shortcut-input").nth(1).click();
  await page.keyboard.press("Control+Alt+Space");
  await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
    shortcut_dictate_enhance: "CtrlLeft+OptLeft+Space",
    enhance_mode: "second_shortcut",
  }]);
});

test("keyboard activation of an idle field still works", async ({ page }) => {
  await openShortcuts(page);
  const input = page.locator(".shortcut-input").first();
  await input.focus();
  await page.keyboard.press("Enter");
  await expect(input).toHaveClass(/recording/);
  await page.keyboard.press("F8");
  await expect.poll(() => page.evaluate(() => (window as any).settings.shortcut_dictate)).toBe("F8");
});

test("Escape cancels and restores global shortcuts without changing settings", async ({ page }) => {
  await openShortcuts(page);
  const input = page.locator(".shortcut-input").first();
  await input.click();
  await page.keyboard.press("Escape");
  await expect(input).not.toHaveClass(/recording/);
  await expect(input).toHaveText("Strg links + Leertaste");
  expect(await page.evaluate(() => (window as any).savedPatches)).toEqual([]);
  await expect.poll(() => page.evaluate(() => (window as any).activeSession)).toBeNull();
});

test("switching fields only records into the newest field", async ({ page }) => {
  await openShortcuts(page);
  const first = page.locator(".shortcut-input").first();
  const second = page.locator(".shortcut-input").nth(1);
  await first.click();
  await second.click();
  await expect(first).not.toHaveClass(/recording/);
  await expect(second).toHaveClass(/recording/);
  await page.keyboard.press("F9");
  await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
    shortcut_dictate_enhance: "F9", enhance_mode: "second_shortcut",
  }]);
  await expect.poll(() => page.evaluate(() => (window as any).activeSession)).toBeNull();
});

test("fast input is retained while the pause command is pending", async ({ page }) => {
  await openShortcuts(page);
  await page.evaluate(() => { (window as any).captureDelay = 200; });
  await page.locator(".shortcut-input").first().click();
  await page.keyboard.press("a");
  await expect.poll(() => page.evaluate(() => (window as any).savedPatches)).toEqual([{
    shortcut_dictate: "A",
  }]);
  await expect.poll(() => page.evaluate(() => (window as any).activeSession)).toBeNull();
});

test("failure to pause global shortcuts is shown instead of silently waiting", async ({ page }) => {
  await openShortcuts(page);
  await page.evaluate(() => { (window as any).failCapture = true; });
  const input = page.locator(".shortcut-input").first();
  await input.click();
  await expect(input).not.toHaveClass(/recording/);
  await expect(page.getByRole("alert")).toContainText("Pause fehlgeschlagen");
  expect(await page.evaluate(() => (window as any).savedPatches)).toEqual([]);
});

test("losing window focus cancels capture", async ({ page }) => {
  await openShortcuts(page);
  const input = page.locator(".shortcut-input").first();
  await input.click();
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await expect(input).not.toHaveClass(/recording/);
  await expect.poll(() => page.evaluate(() => (window as any).activeSession)).toBeNull();
  expect(await page.evaluate(() => (window as any).savedPatches)).toEqual([]);
});

test("timeout is visible and does not leave global shortcuts disabled", async ({ page }) => {
  await openShortcuts(page);
  await page.clock.install();
  const input = page.locator(".shortcut-input").first();
  await input.click();
  await page.clock.fastForward(20_001);
  await expect(input).not.toHaveClass(/recording/);
  await expect(page.getByRole("alert")).toContainText("Keine vollständige Tastenkombination");
  await expect.poll(() => page.evaluate(() => (window as any).activeSession)).toBeNull();
});

test("duplicate shortcut produces a visible conflict, not an endless capture", async ({ page }) => {
  await openShortcuts(page);
  const input = page.locator(".shortcut-input").nth(1);
  await input.click();
  await page.keyboard.press("Control+Space");
  await expect(input).not.toHaveClass(/recording/);
  await expect(page.getByText("Dieser Shortcut ist schon vergeben.")).toBeVisible();
  expect(await page.evaluate(() => (window as any).savedPatches)).toEqual([]);
});
