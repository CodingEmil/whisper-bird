import { useEffect, useState } from "react";
import { Check, Download } from "lucide-react";
import { api, formatBytes } from "../lib/api";
import { Dots, Progress, Segmented, useApp } from "../lib/app";
import { useModels } from "./Models";
import { WithShortcut } from "./Home";
import { Bird } from "../lib/bird";

const CHOICES = ["parakeet-ultra","Qwen3-ASR-1.7B", "whisper-large-v3-turbo", "parakeet-unified-en-0.6b"];

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const { t, settings: s, update, lang } = useApp();
  const { models, progress } = useModels();
  const [choice, setChoice] = useState(s.asr_model || CHOICES[0]);
  const [step, setStep] = useState<"model" | "download" | "try">("model");

  const model = models.find((m) => m.id === choice);
  const p = progress[choice];

  useEffect(() => {
    if (step === "download" && (model?.installed || p?.state === "done")) setStep("try");
  }, [step, model?.installed, p?.state]);

  const finish = async () => {
    await api.finishOnboarding();
    onDone();
  };

  return (
    <div className="onboarding">
      <div className="onboarding-card">
        <div className="onboarding-top">
          <div className="brand">
            <Bird size={56} />
            <span className="brand-name">{t("app.name")}</span>
          </div>
          <Segmented
            value={s.ui_language}
            label="Sprache, language"
            options={[
              ["de", "Deutsch"],
              ["en", "English"],
            ]}
            onChange={(ui_language) => update({ ui_language })}
          />
        </div>

        {step === "model" && (
          <>
            <h1 className="page-title">{t("onb.welcome")}</h1>
            <p className="page-sub">{t("onb.welcome_text")}</p>
            <h2 className="card-title" id="onb-models">{t("onb.choose_model")}</h2>
            <div className="choices" role="group" aria-labelledby="onb-models">
              {CHOICES.map((id) => {
                const m = models.find((x) => x.id === id);
                if (!m) return null;
                return (
                  <button type="button" key={id} className="choice" aria-pressed={choice === id} onClick={() => setChoice(id)}>
                    <span className="choice-head">
                      <span className="hstack wrap">
                        <strong>{m.name}</strong>
                        {m.recommended && <span className="badge accent">{t("common.recommended")}</span>}
                        {m.installed && <span className="badge success"><Check size={11} /> {t("common.installed")}</span>}
                      </span>
                      <span className="small muted num">{formatBytes(m.size)}</span>
                    </span>
                    <span className="choice-desc block">
                      {lang === "de" ? m.description_de : m.description_en}
                    </span>
                    <span className="model-meta">
                      <span>{t("common.speed")} <Dots value={m.speed} /></span>
                      <span>{t("common.accuracy")} <Dots value={m.accuracy} /></span>
                    </span>
                  </button>
                );
              })}
            </div>
            <div className="onboarding-actions">
              <button type="button" className="btn ghost" onClick={finish}>{t("onb.skip")}</button>
              <button
                type="button"
                className="btn primary large"
                onClick={async () => {
                  await update({ asr_model: choice });
                  if (model?.installed) setStep("try");
                  else {
                    await api.downloadModel(choice);
                    setStep("download");
                  }
                }}
              >
                <Download size={17} /> {t("onb.download")}
              </button>
            </div>
          </>
        )}

        {step === "download" && model && (
          <>
            <h1 className="page-title">{model.name}</h1>
            <p className="page-sub" role="status">
              {p?.state === "verifying" ? t("common.verifying") : t("common.downloading")}
            </p>
            <Progress value={p && p.total ? p.downloaded / p.total : 0} />
            <div className="download-state">
              {formatBytes(p?.downloaded ?? 0)} / {formatBytes(model.size)}
            </div>
            {p?.state === "error" && (
              <div className="notice danger" role="alert">
                <div className="spacer">{p.error}</div>
                <button type="button" className="btn small" onClick={() => api.downloadModel(choice)}>{t("common.download")}</button>
              </div>
            )}
          </>
        )}

        {step === "try" && (
          <>
            <h1 className="page-title">{t("onb.ready")}</h1>
            <p className="page-sub">
              <WithShortcut text={t("onb.shortcut_text")} shortcut={s.shortcut_dictate} />
            </p>
            <textarea className="textarea" rows={5} autoFocus aria-label={t("home.try")} placeholder={t("home.try_placeholder")} />
            <div className="onboarding-actions end">
              <button type="button" className="btn primary large" onClick={finish}>
                <Check size={17} /> {t("onb.done")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
