/** KeyboardEvent.code -> handy-keys spelling (physical keys, independent of layout). */
const MODIFIERS: Record<string, string> = {
  ControlLeft: "CtrlLeft", ControlRight: "CtrlRight",
  AltLeft: "OptLeft", AltRight: "OptRight",
  ShiftLeft: "ShiftLeft", ShiftRight: "ShiftRight",
  MetaLeft: "CmdLeft", MetaRight: "CmdRight",
};

const KEYS: Record<string, string> = {
  Space: "Space", Enter: "Return", Tab: "Tab", Escape: "Escape",
  Backspace: "Delete", Delete: "ForwardDelete", Insert: "Insert",
  Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
  ArrowLeft: "LeftArrow", ArrowRight: "RightArrow", ArrowUp: "UpArrow", ArrowDown: "DownArrow",
  Minus: "Minus", Equal: "Equal", BracketLeft: "LeftBracket", BracketRight: "RightBracket",
  Backslash: "Backslash", Semicolon: "Semicolon", Quote: "Quote", Comma: "Comma",
  Period: "Period", Slash: "Slash", Backquote: "Grave", IntlBackslash: "Section",
  IntlYen: "JisYen", IntlRo: "JisUnderscore", KanaMode: "JisKana",
  NumpadDecimal: "KeypadDecimal", NumpadMultiply: "KeypadMultiply", NumpadAdd: "KeypadPlus",
  NumpadDivide: "KeypadDivide", NumpadEnter: "KeypadEnter", NumpadSubtract: "KeypadMinus",
  NumpadEqual: "KeypadEquals", NumpadComma: "KeypadComma", NumpadClear: "KeypadClear",
  CapsLock: "CapsLock", ScrollLock: "ScrollLock", NumLock: "NumLock",
  PrintScreen: "PrintScreen", Pause: "Pause", ContextMenu: "ContextMenu",
  MediaPlayPause: "PlayPause", MediaStop: "Stop", MediaTrackPrevious: "PrevTrack", MediaTrackNext: "NextTrack",
};

const FAMILIES = [
  ["ctrlKey", "Ctrl"], ["altKey", "Opt"], ["shiftKey", "Shift"], ["metaKey", "Cmd"],
] as const;
const MODIFIER_ORDER = FAMILIES.flatMap(([, name]) => [name, `${name}Left`, `${name}Right`]);

function keyName(event: KeyboardEvent): string | null {
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3);
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^Numpad[0-9]$/.test(event.code)) return `Keypad${event.code.slice(6)}`;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.code)) return event.code;
  return KEYS[event.code] ?? null;
}

export type CaptureUpdate =
  | { kind: "live"; shortcut: string }
  | { kind: "done"; shortcut: string }
  | { kind: "cancel" }
  | { kind: "unsupported" };

/**
 * Foreground capture: no transient global hook, no events from other applications.
 * The shortcut is whatever was held together when the first key is released, so a
 * modifier that arrives a few ms after the key ("pressed at the same time") still counts.
 */
export class ShortcutCapture {
  private heldModifiers = new Set<string>();
  private heldKeys = new Set<string>();
  private peak = new Set<string>();
  private key: string | null = null;
  private released = false;
  private unsupported = false;

  handle(event: KeyboardEvent, down: boolean): CaptureUpdate | null {
    if (event.isComposing) return null;
    const modifier = MODIFIERS[event.code];
    const held = modifier ? this.heldModifiers : this.heldKeys;
    const name = modifier ?? event.code;
    // Only keys pressed during this capture count (not e.g. the Enter that opened it).
    if (!down && held.has(name)) this.released = true;
    if (down) held.add(name);
    else held.delete(name);
    const modifiers = FAMILIES.flatMap(([flag, name]) => {
      if (!event[flag]) {
        for (const held of this.heldModifiers) {
          if (held.startsWith(name)) this.heldModifiers.delete(held);
        }
        return [];
      }
      const sides = [...this.heldModifiers].filter((held) => held.startsWith(name));
      // Also handle a modifier that was already held when capture began.
      return sides.length ? sides : [name];
    });

    if (down) {
      if (event.code === "Escape" && !modifiers.length) return { kind: "cancel" };
      if (this.released) return null; // The combination is fixed, ignore repeats.
      if (!modifier) {
        if (this.key) return null; // Keep the first key.
        const key = keyName(event);
        if (!key) {
          this.unsupported = true;
          return { kind: "unsupported" };
        }
        this.key = key;
      }
      for (const mod of modifiers) this.peak.add(mod);
      return { kind: "live", shortcut: this.shortcut() };
    }

    if (modifiers.length || this.heldKeys.size) return null;
    if (this.key) return { kind: "done", shortcut: this.shortcut() };
    if (this.unsupported) {
      this.unsupported = false;
      this.released = false;
      this.peak.clear();
    } else if (this.peak.size) {
      return { kind: "done", shortcut: this.shortcut() };
    }
    return null;
  }

  private shortcut(): string {
    const modifiers = MODIFIER_ORDER.filter((mod) => this.peak.has(mod));
    return (this.key ? [...modifiers, this.key] : modifiers).join("+");
  }
}
