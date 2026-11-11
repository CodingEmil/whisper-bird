import { useEffect, useState } from "react";
import { Check, Download, FlaskConical, LoaderCircle, Trash2, X } from "lucide-react";
import { api, formatBytes, type DownloadProgress, type ModelKind, type ModelStatus } from "../lib/api";
import { Card, Dots, Progress, Row, Select, useApp, useEvent } from "../lib/app";
import { useConfirm } from "../lib/confirm";
import type { Accelerator } from "../lib/api";

export function useModels() {
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const refresh = () => api.listModels().then(setModels);
  useEffect(() => {
    refresh();
  }, []);
  useEvent<DownloadProgress>("download-progress", (p) => {
    setProgress((prev) => ({ ...prev, [p.id]: p }));
    if (p.state === "done" || p.state === "error" || p.state === "cancelled") refresh();
  });
  return { models, progress, refresh };
}

export function ModelRow({
  model,
  progress,
  selected,
  onSelect,
  onChanged,
}: {
  model: ModelStatus;
  progress?: DownloadProgress;
  selected?: boolean;
  onSelect?: () => void;
  onChanged: () => void;
}) {
  const { t, lang } = useApp();
  const confirm = useConfirm();
  const downloading = model.downloading || progress?.state === "downloading" || progress?.state === "verifying";
  const ratio = progress && progress.total > 0 ? progress.downloaded / progress.total : 0;
  const langs = model.languages.length;

  return (
    <div className="row top">
      <div className="row-text">
        <div className="hstack wrap">
          <span className="row-label">{model.name}</span>
          {model.recommended && <span className="badge accent">{t("common.recommended")}</span>}
        </div>
        <div className="row-hint">{lang === "de" ? model.description_de : model.description_en}</div>
        <div className="model-meta">
          {model.kind === "asr" && (
            <>
              <span>
                {t("common.speed")} <Dots value={model.speed} />
              </span>
              <span>
                {t("common.accuracy")} <Dots value={model.accuracy} />
              </span>
              <span>
                {langs} {t("common.languages")}
              </span>
            </>
          )}
          <span className="num">{formatBytes(model.size)}</span>
        </div>
        {downloading && (
          <div className="download">
            <Progress value={ratio} />
            <div className="small muted">
              {progress?.state === "verifying"
                ? t("common.verifying")
                : `${formatBytes(progress?.downloaded ?? 0)} / ${formatBytes(model.size)}`}
            </div>
          </div>
        )}
        {progress?.state === "error" && (
          <div className="notice danger small download">
            {progress.error}
          </div>
        )}
      </div>
      <div className="row-control">
        {model.installed ? (
          <>
            {onSelect &&
              (selected ? (
                <span className="badge success">
                  <Check size={12} /> {t("common.active")}
                </span>
              ) : (
                <button type="button" className="btn small" onClick={onSelect}>
                  {t("common.use")}
                </button>
              ))}
            <button
              type="button"
              className="btn ghost icon danger"
              title={t("common.delete")}
              aria-label={t("common.delete")}
              onClick={async () => {
                const ok = await confirm({
                  title: t("models.delete_confirm"),
                  body: t("models.delete_body", { name: model.name }),
                  action: t("common.delete"),
                });
                if (ok) {
                  await api.deleteModel(model.id);
                  onChanged();
                }
              }}
            >
              <Trash2 size={15} />
            </button>
          </>
        ) : downloading ? (
          <button type="button" className="btn small" onClick={() => api.cancelDownload(model.id)}>
            <X size={14} /> {t("common.cancel")}
          </button>
        ) : (
          <button
            type="button"
            className="btn small"
            onClick={async () => {
              await api.downloadModel(model.id);
              onChanged();
            }}
          >
            <Download size={14} /> {t("common.download")}
          </button>
        )}
        {downloading && <LoaderCircle size={16} className="spin muted" />}
      </div>
    </div>
  );
}

/** OpenAI-kompatible Spracherkennung: ausfüllen, speichern, dann eingeklappt. */
function RemoteAsr() {
  const { t, settings: s, update } = useApp();
  const [editing, setEditing] = useState(!s.asr_remote_url);
  const saved = () => ({ url: s.asr_remote_url ?? "", key: s.asr_remote_key ?? "", model: s.asr_remote_model ?? "" });
  const [form, setForm] = useState(saved);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);
  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value });
  const trimmed = { url: form.url.trim(), key: form.key.trim(), model: form.model.trim() || "whisper-1" };

  const startEdit = () => {
    setForm(saved());
    setTestResult(null);
    setEditing(true);
  };
  const saveForm = async () => {
    await update({ asr_remote_url: trimmed.url, asr_remote_key: trimmed.key, asr_remote_model: trimmed.model });
    setTestResult(null);
    setEditing(false);
  };
  const runTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult({ ok: true, text: await api.asrRemoteTest(trimmed.url, trimmed.key, trimmed.model) });
    } catch (e) {
      setTestResult({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
    }
  };

  if (!editing) {
    return (
      <Card title={t("models.remote")} sub={t("models.remote_hint")}>
        <Row label={<span className="mono">{s.asr_remote_url}</span>} hint={s.asr_remote_model}>
          <span className="badge success">
            <Check size={12} /> {t("models.remote_saved")}
          </span>
          <button type="button" className="btn small" onClick={startEdit}>
            {t("common.change")}
          </button>
          <button
            type="button"
            className="btn ghost icon danger"
            title={t("common.remove")}
            aria-label={t("common.remove")}
            onClick={() => {
              update({ asr_remote_url: "", asr_remote_key: "" });
              setForm({ url: "", key: "", model: s.asr_remote_model });
              setEditing(true);
            }}
          >
            <Trash2 size={15} />
          </button>
        </Row>
      </Card>
    );
  }

  return (
    <Card title={t("models.remote")} sub={t("models.remote_hint")}>
      <Row label={t("enh.base_url")} stack>
        <input
          className="input wide mono"
          type="text"
          placeholder="https://api.openai.com/v1"
          aria-label={t("enh.base_url")}
          value={form.url}
          onChange={set("url")}
        />
      </Row>
      <Row label={t("enh.api_key")} stack>
        <input
          className="input wide mono"
          type="password"
          autoComplete="off"
          aria-label={t("enh.api_key")}
          value={form.key}
          onChange={set("key")}
        />
      </Row>
      <Row label={t("enh.api_model")} stack>
        <input
          className="input wide mono"
          type="text"
          placeholder="whisper-1"
          aria-label={t("enh.api_model")}
          value={form.model}
          onChange={set("model")}
        />
      </Row>
      <Row label="">
        <button type="button" className="btn" disabled={testing || !trimmed.url} aria-busy={testing} onClick={runTest}>
          {testing ? <LoaderCircle size={15} className="spin" /> : <FlaskConical size={15} />}
          {testing ? t("enh.test_running") : t("common.test")}
        </button>
        {s.asr_remote_url && (
          <button type="button" className="btn" onClick={() => setEditing(false)}>
            {t("common.cancel")}
          </button>
        )}
        <button type="button" className="btn primary" disabled={!trimmed.url} onClick={saveForm}>
          {t("common.save")}
        </button>
      </Row>
      {testResult && (
        <div className="card-inset">
          <div className={`notice ${testResult.ok ? "success" : "danger"}`}>
            <div className="selectable pre">
              {testResult.ok ? <strong>{t("models.remote_ok")} </strong> : null}
              {testResult.text}
            </div>
          </div>
        </div>
      )}
    </Card>
  );
}

export default function Models() {
  const { t, settings: s, update } = useApp();
  const { models, progress, refresh } = useModels();
  const [devices, setDevices] = useState<string[]>([]);
  useEffect(() => {
    api.appInfo().then((i) => setDevices(i.devices));
  }, []);

  const byKind = (k: ModelKind[]) => models.filter((m) => k.includes(m.kind));
  const installed = byKind(["asr"]).filter((m) => m.installed).map((m): [string, string] => [m.id, m.name]);
  if (s.asr_remote_url) installed.push(["remote", `Remote (${s.asr_remote_model})`]);

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("models.title")}</h1>
      </header>
      <p className="page-sub">{t("models.subtitle")}</p>

      {installed.length > 0 && (
        <Card title={t("models.usage")}>
          <Row label={t("models.usage_dictation")} hint={t("models.usage_dictation_hint")}>
            <Select value={s.asr_model} options={installed} onChange={(asr_model) => update({ asr_model })} />
          </Row>
          <Row label={t("models.usage_meeting")} hint={t("models.usage_meeting_hint")}>
            <Select
              value={s.meeting_asr_model}
              options={[["", t("models.usage_same")], ...installed]}
              onChange={(meeting_asr_model) => update({ meeting_asr_model })}
            />
          </Row>
        </Card>
      )}

      <Card title={t("models.asr")}>
        {byKind(["asr"]).map((m) => (
          <ModelRow key={m.id} model={m} progress={progress[m.id]} onChanged={refresh} />
        ))}
      </Card>

      <RemoteAsr />

      <Card title={t("models.gpu")}>
        <Row label={t("adv.accelerator")} hint={devices.join(", ")}>
          <Select<Accelerator>
            value={s.accelerator}
            options={[
              ["auto", t("adv.accel_auto")],
              ["gpu", t("adv.accel_gpu")],
              ["cpu", t("adv.accel_cpu")],
            ]}
            onChange={(accelerator) => update({ accelerator })}
          />
        </Row>
      </Card>

      <Card title={t("models.llm")} sub={t("models.llm_hint")}>
        {byKind(["llm"]).map((m) => (
          <ModelRow
            key={m.id}
            model={m}
            progress={progress[m.id]}
            selected={s.enhance_local_model === m.id}
            onSelect={() => update({ enhance_local_model: m.id })}
            onChanged={refresh}
          />
        ))}
      </Card>

      <Card title={t("models.helpers")} sub={t("models.helpers_hint")}>
        {byKind(["vad", "diarize"]).map((m) => (
          <ModelRow key={m.id} model={m} progress={progress[m.id]} onChanged={refresh} />
        ))}
      </Card>
    </>
  );
}
