import { useEffect, useState } from "react";
import { Check, Download } from "lucide-react";
import { api, formatBytes } from "../lib/api";
import { Dots, Progress, Segmented, useApp } from "../lib/app";
import { useModels } from "./Models";
import { WithShortcut } from "./Home";
import iconUrl from "../../src-tauri/icons/128x128@2x.png";

const CHOICES = ["parakeet-tdt-0.6b-v3", "Qwen3-ASR-1.7B", "whisper-large-v3-turbo", "parakeet-unified-en-0.6b"];

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
      <div className="onboarding-card fade-in">
        <div className="hstack" style={{ justifyContent: "space-between", marginBottom: 18 }}>
          <img src={iconUrl} alt="" width={56} height={56} />
          <Segmented
            value={s.ui_language}
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
            <p className="page-sub" style={{ marginBottom: 20 }}>{t("onb.welcome_text")}</p>
            <div className="small muted" style={{ marginBottom: 8, fontWeight: 600 }}>{t("onb.choose_model")}</div>
            <div className="vstack" style={{ gap: 8 }}>
              {CHOICES.map((id) => {
                const m = models.find((x) => x.id === id);
                if (!m) return null;
                return (
                  <button key={id} className={`choice ${choice === id ? "active" : ""}`} onClick={() => setChoice(id)}>
                    <div className="hstack" style={{ justifyContent: "space-between" }}>
                      <span className="hstack">
                        <strong>{m.name}</strong>
                        {m.recommended && <span className="badge accent">{t("common.recommended")}</span>}
                        {m.installed && <span className="badge success"><Check size={11} /> {t("common.installed")}</span>}
                      </span>
                      <span className="small muted">{formatBytes(m.size)}</span>
                    </div>
                    <div className="small muted" style={{ textAlign: "left", marginTop: 2 }}>
                      {lang === "de" ? m.description_de : m.description_en}
                    </div>
                    <div className="hstack small muted" style={{ gap: 14, marginTop: 6 }}>
                      <span className="hstack" style={{ gap: 6 }}>{t("common.speed")} <Dots value={m.speed} /></span>
                      <span className="hstack" style={{ gap: 6 }}>{t("common.accuracy")} <Dots value={m.accuracy} /></span>
                    </div>
                  </button>
                );
              })}
            </div>
            <div className="hstack" style={{ marginTop: 22, justifyContent: "space-between" }}>
              <button className="btn ghost" onClick={finish}>{t("onb.skip")}</button>
              <button
                className="btn gradient large"
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
            <p className="page-sub" style={{ marginBottom: 22 }}>
              {p?.state === "verifying" ? t("common.verifying") : t("common.downloading")}
            </p>
            <Progress value={p && p.total ? p.downloaded / p.total : 0} />
            <div className="small muted" style={{ marginTop: 8 }}>
              {formatBytes(p?.downloaded ?? 0)} / {formatBytes(model.size)}
            </div>
            {p?.state === "error" && (
              <div className="notice danger" style={{ marginTop: 16 }}>
                {p.error}
                <div className="spacer" />
                <button className="btn small" onClick={() => api.downloadModel(choice)}>{t("common.download")}</button>
              </div>
            )}
          </>
        )}

        {step === "try" && (
          <>
            <h1 className="page-title">{t("onb.ready")}</h1>
            <p className="page-sub" style={{ marginBottom: 18 }}>
              <WithShortcut text={t("onb.shortcut_text")} shortcut={s.shortcut_dictate} />
            </p>
            <textarea className="textarea" rows={5} autoFocus placeholder={t("home.try_placeholder")} />
            <div className="hstack" style={{ marginTop: 22, justifyContent: "flex-end" }}>
              <button className="btn gradient large" onClick={finish}>
                <Check size={17} /> {t("onb.done")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
