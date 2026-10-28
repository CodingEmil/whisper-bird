import { useEffect, useState } from "react";
import { Check, Download, LoaderCircle, Trash2, X } from "lucide-react";
import { api, formatBytes, type DownloadProgress, type ModelKind, type ModelStatus } from "../lib/api";
import { Card, Dots, Progress, Row, Select, useApp, useEvent } from "../lib/app";
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
                if (confirm(t("models.delete_confirm"))) {
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

export default function Models() {
  const { t, settings: s, update } = useApp();
  const { models, progress, refresh } = useModels();
  const [devices, setDevices] = useState<string[]>([]);

  useEffect(() => {
    api.appInfo().then((i) => setDevices(i.devices));
  }, []);

  const byKind = (k: ModelKind[]) => models.filter((m) => k.includes(m.kind));

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("models.title")}</h1>
      </header>
      <p className="page-sub">{t("models.subtitle")}</p>

      <Card title={t("models.asr")}>
        {byKind(["asr"]).map((m) => (
          <ModelRow
            key={m.id}
            model={m}
            progress={progress[m.id]}
            selected={s.asr_model === m.id}
            onSelect={() => update({ asr_model: m.id })}
            onChanged={refresh}
          />
        ))}
      </Card>

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
