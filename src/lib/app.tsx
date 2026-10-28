import { createContext, Fragment, useCallback, useContext, useEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { api, formatShortcut, on, type Settings, type ShortcutError } from "./api";
import { KeyTracker, ShortcutCapture } from "./shortcut-capture";
import { translate, type TKey } from "./i18n";

interface AppCtx {
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<ShortcutError[]>;
  t: (key: TKey, vars?: Record<string, string | number>) => string;
  lang: string;
}

const Ctx = createContext<AppCtx | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    api.getSettings().then(setSettings);
    return on<Settings>("settings-changed", setSettings);
  }, []);

  const update = useCallback(async (patch: Partial<Settings>) => {
    setSettings((s) => (s ? { ...s, ...patch } : s));
    const res = await api.setSettings(patch);
    setSettings(res.settings);
    return res.shortcut_errors;
  }, []);

  if (!settings) return null;
  const lang = settings.ui_language;
  const t = (key: TKey, vars?: Record<string, string | number>) => translate(lang, key, vars);
  return <Ctx.Provider value={{ settings, update, t, lang }}>{children}</Ctx.Provider>;
}

export function useApp(): AppCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("SettingsProvider fehlt");
  return ctx;
}

// ---------- Bausteine ----------

/** Die Beschriftung der Zeile, damit Steuerelemente darin ihren Namen tragen. */
const RowLabel = createContext<string | undefined>(undefined);
export const useRowLabel = () => useContext(RowLabel);

export function Toggle({ value, onChange, disabled }: { value: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={value}
      aria-labelledby={useRowLabel()}
      className={`toggle ${value ? "on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!value)}
    />
  );
}

/** Eine Gruppe: Überschrift außen, der Inhalt auf einer getönten Fläche. */
export function Card({ title, sub, actions, children }: { title?: ReactNode; sub?: ReactNode; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="card">
      {(title || actions) && (
        <div className="card-header">
          <div>
            {title && <h2 className="card-title">{title}</h2>}
            {sub && <p className="card-sub">{sub}</p>}
          </div>
          {actions && <div className="hstack">{actions}</div>}
        </div>
      )}
      <div className="card-well">{children}</div>
    </section>
  );
}

export function Row({ label, hint, children, stack }: { label: ReactNode; hint?: ReactNode; children?: ReactNode; stack?: boolean }) {
  const id = useId();
  return (
    <div className={`row ${stack ? "stack" : ""}`}>
      <div className="row-text">
        <div className="row-label" id={id}>{label}</div>
        {hint && <div className="row-hint">{hint}</div>}
      </div>
      {children && (
        <div className="row-control">
          <RowLabel.Provider value={id}>{children}</RowLabel.Provider>
        </div>
      )}
    </div>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: [T, ReactNode][]; onChange: (v: T) => void; label?: string }) {
  const labelledBy = useRowLabel();
  return (
    <div className="segmented" role="group" aria-labelledby={label ? undefined : labelledBy} aria-label={label}>
      {options.map(([v, text]) => (
        <button key={v} type="button" aria-pressed={v === value} onClick={() => onChange(v)}>
          {text}
        </button>
      ))}
    </div>
  );
}

export function Select<T extends string>({ value, options, onChange, style, label }: { value: T; options: [T, string][]; onChange: (v: T) => void; style?: CSSProperties; label?: string }) {
  const labelledBy = useRowLabel();
  return (
    <span className="select-wrap">
      <select
        className="select"
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        style={style}
        aria-labelledby={label ? undefined : labelledBy}
        aria-label={label}
      >
        {options.map(([v, text]) => (
          <option key={v} value={v}>
            {text}
          </option>
        ))}
      </select>
      <ChevronDown size={15} />
    </span>
  );
}

export function Dots({ value, max = 5 }: { value: number; max?: number }) {
  return (
    <span className="dots" role="img" aria-label={`${value} / ${max}`}>
      {Array.from({ length: max }, (_, i) => (
        <span key={i} className={i < value ? "on" : ""} />
      ))}
    </span>
  );
}

export function Progress({ value }: { value: number }) {
  return (
    <div className="progress">
      <div style={{ transform: `scaleX(${Math.max(0, Math.min(1, value))})` }} />
    </div>
  );
}

/** Tastenkappen für einen formatierten Shortcut („Strg links + Leertaste“); der Text bleibt derselbe. */
function Keycaps({ text }: { text: string }) {
  return (
    <span className="kbd">
      {text.split(" + ").map((k, i) => (
        <Fragment key={i}>
          {i > 0 && " + "}
          <kbd>{k}</kbd>
        </Fragment>
      ))}
    </span>
  );
}

/** Ein Shortcut als Tastenkappen; die Größe folgt dem umgebenden Text. */
export function Kbd({ shortcut }: { shortcut: string }) {
  const { lang, t } = useApp();
  if (!shortcut) return <span className="muted">{t("common.not_set")}</span>;
  return <Keycaps text={formatShortcut(shortcut, lang)} />;
}

interface CaptureSession {
  id: string;
  keys: ShortcutCapture;
  ready: Promise<void>;
  timer: number;
}

const CAPTURE_START = "whisper-shortcut-capture-start";

/** Foreground keyboard events capture the keys; the backend only pauses global actions. */
export function ShortcutInput({ value, onChange, allowClear }: { value: string; onChange: (v: string) => void | Promise<void>; allowClear?: boolean }) {
  const { t, lang } = useApp();
  const [recording, setRecording] = useState(false);
  const [live, setLive] = useState("");
  const [error, setError] = useState("");
  const active = useRef<CaptureSession | null>(null);
  const mounted = useRef(false);
  const latest = useRef({ t, onChange });
  latest.current = { t, onChange };

  const finish = useCallback(async (session: CaptureSession, shortcut?: string, message = "") => {
    if (active.current !== session) return;
    active.current = null; // Keyup, blur and timeout must not save/stop twice.
    window.clearTimeout(session.timer);
    if (mounted.current) {
      setRecording(false);
      setLive("");
      setError(message);
    }
    try {
      // A fast keystroke can complete before the pause command returns.
      await session.ready;
      await api.stopShortcutCapture(session.id);
      if (shortcut !== undefined) await latest.current.onChange(shortcut);
    } catch (e) {
      if (mounted.current) setError(latest.current.t("sc.capture_failed", { error: String(e) }));
      void api.stopShortcutCapture(session.id).catch(() => {});
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    const cancel = () => { if (active.current) void finish(active.current); };
    const otherCapture = (e: Event) => {
      if (active.current && (e as CustomEvent<string>).detail !== active.current.id) cancel();
    };
    const keyboard = (e: KeyboardEvent) => {
      const session = active.current;
      if (!session) return;
      // In particular, do not let Space/Enter click the capture button again.
      e.preventDefault();
      e.stopPropagation();
      const update = session.keys.handle(e, e.type === "keydown");
      if (!update) return;
      if (update.kind === "cancel") void finish(session);
      else if (update.kind === "unsupported") setError(latest.current.t("sc.capture_unsupported"));
      else if (update.kind === "done") void finish(session, update.shortcut);
      else {
        setLive(update.shortcut);
        setError("");
      }
    };
    window.addEventListener("keydown", keyboard, true);
    window.addEventListener("keyup", keyboard, true);
    window.addEventListener("blur", cancel);
    window.addEventListener(CAPTURE_START, otherCapture);
    return () => {
      mounted.current = false;
      cancel();
      window.removeEventListener("keydown", keyboard, true);
      window.removeEventListener("keyup", keyboard, true);
      window.removeEventListener("blur", cancel);
      window.removeEventListener(CAPTURE_START, otherCapture);
    };
  }, [finish]);

  const start = () => {
    if (active.current) { void finish(active.current); return; }
    const session: CaptureSession = {
      id: crypto.randomUUID(), keys: new ShortcutCapture(), ready: Promise.resolve(), timer: 0,
    };
    active.current = session;
    window.dispatchEvent(new CustomEvent(CAPTURE_START, { detail: session.id }));
    setRecording(true);
    setLive("");
    setError("");
    session.ready = api.startShortcutCapture(session.id);
    session.ready.catch(() => { void finish(session); });
    session.timer = window.setTimeout(() => {
      void finish(session, undefined, latest.current.t("sc.capture_timeout"));
    }, 20_000);
  };

  return (
    <div>
      <div className="hstack">
        <button
          type="button"
          className={`shortcut-input ${recording ? "recording" : ""} ${(recording ? live : value) ? "set" : ""}`}
          aria-pressed={recording}
          onClick={start}
          onBlur={() => { if (active.current) void finish(active.current); }}
        >
          {recording ? (live ? <Keycaps text={formatShortcut(live, lang)} /> : t("common.press_keys")) : value ? <Keycaps text={formatShortcut(value, lang)} /> : t("common.not_set")}
        </button>
        {allowClear && value && !recording && (
          <button type="button" className="btn ghost small" onClick={() => onChange("")}>
            {t("common.remove")}
          </button>
        )}
      </div>
      {error && <div className="row-hint error-text" role="alert">{error}</div>}
    </div>
  );
}

/**
 * Hat die WebView den Fokus, ruft Windows den globalen Tastatur-Hook der App nicht auf.
 * Das Fenster meldet seine Tasten deshalb selbst, der Abgleich läuft im Backend.
 */
export function useWindowShortcuts() {
  useEffect(() => {
    const keys = new KeyTracker();
    // Der Reihe nach, damit ein kurzer Druck nicht als Loslassen vor dem Drücken ankommt.
    let queue = Promise.resolve();
    const send = (call: () => Promise<void>) => { queue = queue.then(call).catch(() => {}); };
    const keyboard = (e: KeyboardEvent) => {
      if (e.repeat || e.isComposing) return;
      const down = e.type === "keydown";
      const k = keys.handle(e, down);
      if (k) send(() => api.windowKey(k.modifiers, k.key, down));
    };
    const blur = () => send(api.windowBlur);
    window.addEventListener("keydown", keyboard, true);
    window.addEventListener("keyup", keyboard, true);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", keyboard, true);
      window.removeEventListener("keyup", keyboard, true);
      window.removeEventListener("blur", blur);
      blur();
    };
  }, []);
}

export function useEvent<T>(event: string, handler: (payload: T) => void, deps: unknown[] = []) {
  useEffect(() => on<T>(event, handler), deps);
}

export function CopyButton({ text, small }: { text: string; small?: boolean }) {
  const { t } = useApp();
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      className={`btn ghost ${small ? "small" : ""}`}
      onClick={async () => {
        await api.copyText(text);
        setDone(true);
        setTimeout(() => setDone(false), 1200);
      }}
    >
      {done ? t("common.copied") : t("common.copy")}
    </button>
  );
}
