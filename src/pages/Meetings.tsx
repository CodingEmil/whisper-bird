import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronLeft, FileText, LoaderCircle, Mic, Monitor, Printer, Sparkles, Square, Trash2, Users } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, formatDuration, type Meeting, type MeetingSource, type MeetingSummary, type ModelStatus } from "../lib/api";
import { Card, CopyButton, Row, Segmented, Toggle, useApp, useEvent } from "../lib/app";
import { ModelRow, useModels } from "./Models";

const SPEAKER_TINTS = ["#5b4cf0", "#0891b2", "#c2410c", "#15803d", "#a21caf", "#b45309"];

function speakerName(m: Meeting, i: number) {
  return m.speakers[i] ?? (i >= 0 ? `#${i + 1}` : "");
}

/** Sehr kleiner Markdown-Renderer für die Notizen (Überschriften, Listen, Aufgaben). */
function Notes({ text }: { text: string }) {
  return (
    <div className="notes selectable">
      {text.split("\n").map((line, i) => {
        const l = line.trimEnd();
        if (l.startsWith("## ")) return <h4 key={i}>{l.slice(3)}</h4>;
        if (l.startsWith("# ")) return <h4 key={i}>{l.slice(2)}</h4>;
        const task = l.match(/^\s*[-*] \[( |x)\] (.*)$/i);
        if (task)
          return (
            <div key={i} className="note-task">
              <span className={`note-box ${task[1].toLowerCase() === "x" ? "done" : ""}`} />
              {task[2]}
            </div>
          );
        if (/^\s*[-*] /.test(l)) return <div key={i} className="note-bullet">{l.replace(/^\s*[-*] /, "")}</div>;
        if (!l.trim()) return <div key={i} style={{ height: 6 }} />;
        return <p key={i}>{l}</p>;
      })}
    </div>
  );
}

function Transcript({ m }: { m: Meeting }) {
  // Aufeinanderfolgende Beiträge desselben Sprechers zusammenfassen.
  const blocks: { speaker: number; start: number; text: string }[] = [];
  for (const s of m.segments) {
    const last = blocks[blocks.length - 1];
    if (last && last.speaker === s.speaker) last.text += " " + s.text;
    else blocks.push({ speaker: s.speaker, start: s.start_ms, text: s.text });
  }
  return (
    <div className="vstack selectable" style={{ gap: 14 }}>
      {blocks.map((b, i) => (
        <div key={i} className="transcript-block">
          <div className="hstack small" style={{ marginBottom: 2 }}>
            {b.speaker >= 0 && <span className="speaker-dot" style={{ background: SPEAKER_TINTS[b.speaker % SPEAKER_TINTS.length] }} />}
            <strong>{b.speaker >= 0 ? speakerName(m, b.speaker) : ""}</strong>
            <span className="muted">{formatDuration(b.start)}</span>
          </div>
          <div style={{ whiteSpace: "pre-wrap" }}>{b.text}</div>
        </div>
      ))}
    </div>
  );
}

function transcriptText(m: Meeting) {
  const lines: string[] = [];
  let last = -2;
  for (const s of m.segments) {
    if (s.speaker === last && lines.length) lines[lines.length - 1] += " " + s.text;
    else lines.push(`${s.speaker >= 0 ? speakerName(m, s.speaker) + ": " : ""}${s.text}`);
    last = s.speaker;
  }
  return lines.join("\n\n");
}

function Detail({ id, onBack }: { id: string; onBack: () => void }) {
  const { t, lang } = useApp();
  const [m, setM] = useState<Meeting | null>(null);
  const [tab, setTab] = useState<"notes" | "transcript">("transcript");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<number | null>(null);
  const [progress, setProgress] = useState<number | null>(null);

  useEffect(() => {
    api.meetingGet(id).then((x) => {
      setM(x);
      if (x.notes) setTab("notes");
    });
  }, [id]);
  useEvent<Meeting>("meeting-update", (x) => x.id === id && setM(x), [id]);
  useEvent<[string, number]>("meeting-progress", ([mid, p]) => mid === id && setProgress(p), [id]);

  if (!m) return null;
  const renameSpeaker = async (i: number, name: string) => {
    const speakers = [...m.speakers];
    speakers[i] = name.trim() || speakers[i];
    setM(await api.meetingUpdate(m.id, undefined, speakers));
    setEditing(null);
  };

  const generate = async () => {
    setBusy(true);
    setError(null);
    try {
      setM(await api.meetingNotes(m.id));
      setTab("notes");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const fullText = () => `${m.title}\n${new Date(m.created_at).toLocaleString(lang)}\n\n${m.notes ? m.notes + "\n\n---\n\n" : ""}${transcriptText(m)}`;

  return (
    <>
      <div className="no-print">
        <button className="btn ghost small" onClick={onBack} style={{ marginBottom: 12 }}>
          <ChevronLeft size={15} /> {t("meet.back")}
        </button>
      </div>
      <div className="print-area">
        <header className="page-header">
          <input
            className="title-input"
            defaultValue={m.title}
            key={m.id}
            onBlur={async (e) => e.target.value.trim() && setM(await api.meetingUpdate(m.id, e.target.value.trim()))}
          />
          <p className="page-sub">
            {new Date(m.created_at).toLocaleString(lang, { dateStyle: "full", timeStyle: "short" })} · {formatDuration(m.duration_ms)}
          </p>
        </header>

        {m.status === "processing" && (
          <div className="notice no-print" style={{ marginBottom: 16 }}>
            <LoaderCircle size={16} className="spin" />
            <div className="spacer">{t("meet.processing")}</div>
            {progress !== null && <span>{Math.round(progress * 100)} %</span>}
          </div>
        )}
        {m.error && <div className="notice danger" style={{ marginBottom: 16 }}>{m.error}</div>}

        {m.speakers.length > 0 && (
          <Card title={t("meet.speakers")} sub={<span className="no-print">{t("meet.rename_hint")}</span>}>
            <div className="card-body chips">
              {m.speakers.map((name, i) =>
                editing === i ? (
                  <input
                    key={i}
                    autoFocus
                    className="input"
                    style={{ height: 30, width: 160 }}
                    defaultValue={name}
                    onBlur={(e) => renameSpeaker(i, e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && renameSpeaker(i, e.currentTarget.value)}
                  />
                ) : (
                  <button key={i} className="chip" style={{ paddingRight: 11, cursor: "pointer" }} onClick={() => setEditing(i)}>
                    <span className="speaker-dot" style={{ background: SPEAKER_TINTS[i % SPEAKER_TINTS.length] }} />
                    {name}
                  </button>
                ),
              )}
            </div>
          </Card>
        )}

        <div className="hstack no-print" style={{ marginBottom: 12, flexWrap: "wrap" }}>
          <Segmented
            value={tab}
            options={[
              ["notes", t("meet.notes")],
              ["transcript", t("meet.transcript")],
            ]}
            onChange={setTab}
          />
          <div className="spacer" />
          <button className="btn small" disabled={busy || m.status !== "done"} onClick={generate}>
            {busy ? <LoaderCircle size={14} className="spin" /> : <Sparkles size={14} />}
            {busy ? t("meet.generating") : t("meet.generate_notes")}
          </button>
          <CopyButton text={transcriptText(m)} small />
          <button
            className="btn small"
            onClick={async () => {
              const path = await save({
                defaultPath: `${m.title.replace(/[\\/:*?"<>|]/g, "-")}.txt`,
                filters: [{ name: "Text", extensions: ["txt", "md"] }],
              });
              if (path) await api.saveTextFile(path, fullText());
            }}
          >
            <FileText size={14} /> {t("meet.export_txt")}
          </button>
          <button className="btn small" onClick={() => window.print()}>
            <Printer size={14} /> {t("meet.export_pdf")}
          </button>
          <button
            className="btn ghost icon danger"
            onClick={async () => {
              if (confirm(t("meet.delete_confirm"))) {
                await api.meetingDelete(m.id);
                onBack();
              }
            }}
          >
            <Trash2 size={15} />
          </button>
        </div>
        {error && <div className="notice danger no-print" style={{ marginBottom: 12 }}>{error}</div>}

        {(tab === "notes" || m.notes) && (
          <div className={tab === "notes" ? "" : "print-only"}>
            <Card title={t("meet.notes")}>
              <div className="card-body">
                {m.notes ? <Notes text={m.notes} /> : <div className="muted">–</div>}
              </div>
            </Card>
          </div>
        )}
        <div className={tab === "transcript" ? "" : "print-only"}>
          <Card title={t("meet.transcript")}>
            <div className="card-body">
              <Transcript m={m} />
            </div>
          </Card>
        </div>
      </div>
    </>
  );
}

function Level({ value, icon }: { value: number; icon: ReactNode }) {
  return (
    <div className="hstack" style={{ gap: 8, minWidth: 160 }}>
      {icon}
      <div className="progress" style={{ flex: 1, height: 8 }}>
        <div style={{ width: `${Math.round(value * 100)}%`, transition: "width .1s" }} />
      </div>
    </div>
  );
}

function LiveView({ id }: { id: string }) {
  const { t } = useApp();
  const [m, setM] = useState<Meeting | null>(null);
  const [levels, setLevels] = useState({ mic: 0, system: 0, duration_ms: 0 });
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.meetingGet(id).then(setM);
  }, [id]);
  useEvent<Meeting>("meeting-update", (x) => x.id === id && setM(x), [id]);
  useEvent<{ id: string; mic: number; system: number; duration_ms: number }>("meeting-level", (l) => l.id === id && setLevels(l), [id]);
  useEffect(() => endRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" }), [m?.segments.length]);

  return (
    <Card
      title={
        <span className="hstack">
          <span className="rec-dot" /> {t("meet.recording")} · {formatDuration(levels.duration_ms)}
        </span>
      }
      actions={
        <button className="btn danger" onClick={() => api.meetingStop()}>
          <Square size={14} /> {t("meet.stop")}
        </button>
      }
    >
      <div className="card-body vstack" style={{ gap: 14 }}>
        <div className="hstack" style={{ gap: 24 }}>
          {m?.source !== "system" && <Level value={levels.mic} icon={<Mic size={15} className="muted" />} />}
          {m?.source !== "microphone" && <Level value={levels.system} icon={<Monitor size={15} className="muted" />} />}
        </div>
        <div className="live-transcript selectable">
          {!m || m.segments.length === 0 ? (
            <span className="muted">{t("meet.waiting")}</span>
          ) : (
            m.segments.map((s, i) => (
              <p key={i} className="fade-in">
                <span className="muted small" style={{ marginRight: 8 }}>{formatDuration(s.start_ms)}</span>
                {s.text}
              </p>
            ))
          )}
          <div ref={endRef} />
        </div>
      </div>
    </Card>
  );
}

export default function Meetings() {
  const { t, settings: s, update, lang } = useApp();
  const [list, setList] = useState<MeetingSummary[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { models, progress, refresh } = useModels();

  const load = () => {
    api.meetingList().then(setList);
    api.meetingActive().then(setActive);
  };
  useEffect(load, []);
  useEvent<Meeting>("meeting-update", (m) => {
    if (m.status !== "recording") setActive((a) => (a === m.id ? null : a));
    else setActive(m.id);
    api.meetingList().then(setList);
  });
  useEvent<string>("meeting-error", setError);

  if (selected) return <Detail id={selected} onBack={() => (setSelected(null), load())} />;

  const diarizer = models.find((m: ModelStatus) => m.kind === "diarize");
  const vad = models.find((m: ModelStatus) => m.kind === "vad");

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("meet.title")}</h1>
        <p className="page-sub">{t("meet.subtitle")}</p>
      </header>

      {active ? (
        <LiveView id={active} />
      ) : (
        <Card>
          <Row label={t("meet.source")}>
            <Segmented<MeetingSource>
              value={s.meeting_source}
              options={[
                ["microphone", t("meet.mic")],
                ["system", t("meet.system")],
                ["both", t("meet.both")],
              ]}
              onChange={(meeting_source) => update({ meeting_source })}
            />
          </Row>
          <Row label={t("meet.auto_notes")} hint={t("meet.auto_notes_hint")}>
            <Toggle value={s.meeting_auto_notes} onChange={(meeting_auto_notes) => update({ meeting_auto_notes })} />
          </Row>
          {diarizer && !diarizer.installed && (
            <ModelRow model={diarizer} progress={progress[diarizer.id]} onChanged={refresh} />
          )}
          {vad && !vad.installed && <ModelRow model={vad} progress={progress[vad.id]} onChanged={refresh} />}
          <div className="row">
            <div className="spacer" />
            <button
              className="btn gradient large"
              onClick={async () => {
                setError(null);
                try {
                  setActive(await api.meetingStart());
                } catch (e) {
                  setError(String(e));
                }
              }}
            >
              <Users size={17} /> {t("meet.start")}
            </button>
          </div>
          {error && (
            <div style={{ padding: "0 18px 16px" }}>
              <div className="notice danger">{error}</div>
            </div>
          )}
        </Card>
      )}

      <Card>
        {list.filter((m) => m.id !== active).length === 0 ? (
          <div className="empty">{t("meet.empty")}</div>
        ) : (
          list
            .filter((m) => m.id !== active)
            .map((m) => (
              <div key={m.id} className="row clickable" onClick={() => setSelected(m.id)}>
                <div className="row-text">
                  <div className="row-label">{m.title}</div>
                  <div className="row-hint">
                    {new Date(m.created_at).toLocaleString(lang, { dateStyle: "medium", timeStyle: "short" })} · {formatDuration(m.duration_ms)}
                    {m.speakers > 0 && ` · ${m.speakers} ${t("meet.speakers")}`}
                  </div>
                  {m.preview && <div className="small muted" style={{ marginTop: 4 }}>{m.preview} …</div>}
                </div>
                {m.status === "processing" && <LoaderCircle size={16} className="spin muted" />}
                {m.status === "error" && <span className="badge danger">{t("common.error")}</span>}
              </div>
            ))
        )}
      </Card>
    </>
  );
}
