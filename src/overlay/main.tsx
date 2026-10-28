import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { AlertTriangle, Check, Plus, Sparkles, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, on, type Settings } from "../lib/api";
import { Bird } from "../lib/bird";
import { translate } from "../lib/i18n";
import "./overlay.css";

type Phase = "idle" | "recording" | "transcribing" | "enhancing" | "done" | "warning" | "error" | "files";

interface OverlayState {
  phase: Phase;
  message: string | null;
  enhance: boolean;
  progress: number | null;
  floating: boolean;
}

const BARS = 22;

function Waveform({ levels }: { levels: number[] }) {
  return (
    <div className="wave">
      {levels.map((l, i) => (
        <span key={i} style={{ transform: `scaleY(${Math.max(0.08, Math.min(1, l))})`, opacity: 0.45 + Math.min(1, l) * 0.55 }} />
      ))}
    </div>
  );
}

function IdleIcon({ drop }: { drop: boolean }) {
  const down = useRef<{ x: number; y: number } | null>(null);
  return (
    <div
      className={`bubble ${drop ? "drop" : ""}`}
      onMouseDown={(e) => {
        if (e.button === 0) down.current = { x: e.screenX, y: e.screenY };
      }}
      onMouseMove={(e) => {
        const d = down.current;
        if (d && Math.hypot(e.screenX - d.x, e.screenY - d.y) > 4) {
          down.current = null;
          getCurrentWindow().startDragging();
        }
      }}
      onMouseUp={(e) => {
        if (e.button === 0 && down.current) {
          down.current = null;
          api.dictationToggle(false);
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        api.overlayContextMenu();
      }}
    >
      {drop ? <Plus size={22} /> : <Bird size={34} preen />}
    </div>
  );
}

function Overlay() {
  const [lang, setLang] = useState("de");
  const [state, setState] = useState<OverlayState>({ phase: "idle", message: null, enhance: false, progress: null, floating: true });
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  const [drop, setDrop] = useState(false);
  const [started, setStarted] = useState(Date.now());
  const [now, setNow] = useState(Date.now());
  const t = (k: Parameters<typeof translate>[1]) => translate(lang, k);

  useEffect(() => {
    api.getSettings().then((s) => setLang(s.ui_language));
    const offs = [
      on<Settings>("settings-changed", (s) => setLang(s.ui_language)),
      on<OverlayState>("overlay-state", (s) => {
        setState((prev) => {
          if (s.phase === "recording" && prev.phase !== "recording") {
            setStarted(Date.now());
            setLevels(Array(BARS).fill(0));
          }
          return s;
        });
      }),
      on<number>("audio-level", (l) => setLevels((prev) => [...prev.slice(1), l])),
      on<boolean>("overlay-drop", setDrop),
    ];
    return () => offs.forEach((o) => o());
  }, []);

  useEffect(() => {
    if (state.phase !== "recording") return;
    const id = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(id);
  }, [state.phase]);

  const secs = Math.max(0, Math.floor((now - started) / 1000));
  const timer = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;

  if (state.phase === "idle") {
    return state.floating ? <IdleIcon drop={drop} /> : null;
  }

  return (
    <div
      className={`pill ${state.phase}`}
      onMouseDown={(e) => {
        // Die ganze Pille ist verschiebbar, nur die Knöpfe nicht.
        if (e.button === 0 && !(e.target as HTMLElement).closest("button")) {
          getCurrentWindow().startDragging();
        }
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {state.phase === "recording" && (
        <>
          {/* Der Vogel singt mit: Der Schnabel folgt dem aktuellen Pegel. */}
          <Bird size={30} open={levels[levels.length - 1] * 1.6} />
          {state.enhance && <Sparkles size={14} className="enh" />}
          <Waveform levels={levels} />
          <span className="timer">{timer}</span>
          <button type="button" className="pill-btn" title="Esc" aria-label={t("common.cancel")} onClick={() => api.dictationCancel()}>
            <X size={15} />
          </button>
          <button type="button" className="pill-btn stop" aria-label={t("onb.done")} onClick={() => api.dictationToggle(state.enhance)}>
            <Square size={11} fill="currentColor" />
          </button>
        </>
      )}
      {(state.phase === "transcribing" || state.phase === "enhancing") && (
        <>
          {/* Während der Arbeit pickt der Vogel – das ist die Ladeanzeige. */}
          <Bird size={30} className="peck" />
          <span className="label">{state.phase === "enhancing" ? t("overlay.enhancing") : t("overlay.transcribing")}</span>
          {state.phase === "enhancing" && <Sparkles size={14} className="enh pulse" />}
          <button type="button" className="pill-btn" title="Esc" aria-label={t("common.cancel")} onClick={() => api.dictationCancel()}>
            <X size={15} />
          </button>
        </>
      )}
      {state.phase === "done" && (
        <>
          <Bird size={30} />
          <Check size={18} className="ok" />
          {state.enhance && <Sparkles size={14} className="enh" />}
        </>
      )}
      {(state.phase === "warning" || state.phase === "error") && (
        <>
          <AlertTriangle size={17} className={state.phase === "error" ? "err" : "warn"} />
          <span className="message">{state.message}</span>
        </>
      )}
      {state.phase === "files" && (
        <>
          <Bird size={30} />
          <div className="file-info">
            <span className="file-name">{state.message}</span>
            <div className="file-bar">
              <div style={{ transform: `scaleX(${Math.max(0, Math.min(1, state.progress ?? 0))})` }} />
            </div>
          </div>
          <span className="timer">{Math.round((state.progress ?? 0) * 100)} %</span>
        </>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
