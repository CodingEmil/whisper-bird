import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import {
  ArrowLeft,
  ArrowUp,
  Briefcase,
  Check,
  Copy,
  Languages,
  List,
  ListChecks,
  LoaderCircle,
  Maximize2,
  Mic,
  Minimize2,
  PenLine,
  RotateCcw,
  Smile,
  SpellCheck,
  Sparkles,
  X,
  type LucideIcon,
} from "lucide-react";
import { api, on, type EditAction, type Settings } from "../lib/api";
import { translate, type TKey } from "../lib/i18n";
import "../styles.css";
import "./editmenu.css";

interface OpenPayload {
  selection: string | null;
  app: string;
  actions: EditAction[];
  translate_target: string;
}

const ICONS: Record<string, LucideIcon> = {
  improve: Sparkles,
  grammar: SpellCheck,
  shorten: Minimize2,
  expand: Maximize2,
  professional: Briefcase,
  casual: Smile,
  bullets: List,
  todo: ListChecks,
  translate: Languages,
};

function EditMenu() {
  const [lang, setLang] = useState("de");
  const [session, setSession] = useState<OpenPayload | null>(null);
  const [input, setInput] = useState("");
  const [active, setActive] = useState(-1); // -1 = Eingabefeld
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [lastInstruction, setLastInstruction] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [listening, setListening] = useState(false);
  const [level, setLevel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const t = (k: TKey, v?: Record<string, string>) => translate(lang, k, v);

  useEffect(() => {
    api.getSettings().then((s) => setLang(s.ui_language));
    const offs = [
      on<Settings>("settings-changed", (s) => setLang(s.ui_language)),
      on<OpenPayload>("editmenu-open", (p) => {
        setSession(p);
        setInput("");
        setActive(-1);
        setBusy(false);
        setResult(null);
        setError(null);
        setListening(false);
        setTimeout(() => inputRef.current?.focus(), 30);
      }),
      on<string>("editmenu-voice", (text) => {
        setListening(false);
        if (text) {
          setInput(text);
          run(text);
        }
      }),
      on<number>("audio-level", setLevel),
    ];
    return () => offs.forEach((o) => o());
  }, []);

  const run = async (instruction: string) => {
    if (!instruction.trim()) return;
    setLastInstruction(instruction);
    setBusy(true);
    setError(null);
    try {
      setResult(await api.editmenuRun(instruction));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const actions = session?.actions ?? [];

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (result !== null) setResult(null);
      else api.editmenuClose();
      return;
    }
    if (result !== null) {
      if (e.key === "Enter" && !e.shiftKey && (e.target as HTMLElement).tagName !== "TEXTAREA") {
        e.preventDefault();
        api.editmenuApply(result);
      } else if (e.key === "Enter" && e.ctrlKey) {
        e.preventDefault();
        api.editmenuApply(result);
      }
      return;
    }
    if (busy) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(actions.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(-1, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (active >= 0 && actions[active]) run(actions[active].instruction);
      else run(input);
    }
  };

  const toggleVoice = () => {
    if (listening) {
      api.editmenuVoice(false);
    } else {
      setListening(true);
      api.editmenuVoice(true);
    }
  };

  if (!session) return null;

  return (
    <div className="em" onKeyDown={onKey}>
      <div className="em-head">
        <PenLine size={15} />
        <span>{t("nav.edit")}</span>
        {session.app && <span className="em-app">{session.app}</span>}
        <div className="spacer" />
        <button type="button" className="em-icon" aria-label={t("common.close")} onClick={() => api.editmenuClose()}>
          <X size={15} />
        </button>
      </div>

      {result === null ? (
        <>
          {session.selection ? (
            <div className="em-selection selectable">{session.selection}</div>
          ) : (
            <div className="em-selection empty">{t("editmenu.no_selection")}</div>
          )}

          <div className={`em-input ${active === -1 ? "focus" : ""}`}>
            <input
              ref={inputRef}
              type="text"
              aria-label={t("editmenu.placeholder")}
              value={input}
              disabled={busy}
              placeholder={listening ? t("editmenu.listening") : t("editmenu.placeholder")}
              onChange={(e) => setInput(e.target.value)}
              onFocus={() => setActive(-1)}
            />
            <button
              type="button"
              className={`em-mic ${listening ? "on" : ""}`}
              style={listening ? { boxShadow: `0 0 0 ${2 + level * 8}px color-mix(in oklab, var(--color-accent-2) 35%, transparent)` } : undefined}
              onClick={toggleVoice}
              disabled={busy}
              title="Mic"
              aria-label="Mic"
              aria-pressed={listening}
            >
              <Mic size={15} />
            </button>
            <button type="button" className="em-send" aria-label={t("editmenu.result")} disabled={busy || !input.trim()} aria-busy={busy} onClick={() => run(input)}>
              {busy ? <LoaderCircle size={15} className="spin" /> : <ArrowUp size={15} />}
            </button>
          </div>

          {busy ? (
            <div className="em-busy" role="status">
              <LoaderCircle size={18} className="spin" /> {t("editmenu.working")}
            </div>
          ) : (
            <div className="em-list">
              {actions.map((a, i) => {
                const Icon = ICONS[a.id] ?? Sparkles;
                const label = a.id === "translate" && session.translate_target ? `${a.label} → ${session.translate_target}` : a.label;
                return (
                  <button
                    type="button"
                    key={a.id}
                    className={`em-item ${active === i ? "active" : ""}`}
                    onMouseEnter={() => setActive(i)}
                    onClick={() => run(a.instruction)}
                  >
                    <Icon size={15} />
                    {label}
                  </button>
                );
              })}
            </div>
          )}
          {error && <div className="notice danger small em-error" role="alert">{error}</div>}
          <div className="em-foot">
            {t("editmenu.hint").split(", ").map((part) => {
              const [key, ...rest] = part.split(" ");
              return (
                <span key={part}>
                  <kbd>{key}</kbd> {rest.join(" ")}
                </span>
              );
            })}
          </div>
        </>
      ) : (
        <>
          <div className="em-result-label">{t("editmenu.result")}</div>
          <textarea className="em-result selectable" aria-label={t("editmenu.result")} value={result} onChange={(e) => setResult(e.target.value)} autoFocus />
          <div className="em-actions">
            <button type="button" className="btn ghost small" aria-label={t("meet.back")} onClick={() => setResult(null)}>
              <ArrowLeft size={14} />
            </button>
            <button type="button" className="btn ghost small" onClick={() => run(lastInstruction)}>
              <RotateCcw size={14} /> {t("editmenu.retry")}
            </button>
            <div className="spacer" />
            <button type="button" className="btn small" onClick={() => api.editmenuCopy(result)}>
              <Copy size={14} /> {t("common.copy")}
            </button>
            <button type="button" className="btn small primary" onClick={() => api.editmenuApply(result)}>
              <Check size={14} /> {session.selection ? t("editmenu.replace") : t("editmenu.insert")}
              <span className="kbd-mini">⏎</span>
            </button>
          </div>
        </>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <EditMenu />
  </React.StrictMode>,
);
