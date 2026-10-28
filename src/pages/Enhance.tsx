import { useEffect, useState } from "react";
import { AlertTriangle, Check, Download, FlaskConical, LoaderCircle } from "lucide-react";
import { api, formatBytes, type DownloadProgress, type EnhanceMode, type EnhanceProvider, type LlamaStatus } from "../lib/api";
import { Card, Progress, Row, Segmented, Select, useApp, useEvent } from "../lib/app";
import { ModelRow, useModels } from "./Models";

const CLI_PRESETS: Record<string, string> = {
  claude: "claude -p --model haiku",
  codex: "codex exec --skip-git-repo-check -",
  gemini: 'gemini -m gemini-2.5-flash -p " "',
};

export default function Enhance() {
  const { t, settings: s, update } = useApp();
  const { models, progress, refresh } = useModels();
  const [llama, setLlama] = useState<LlamaStatus | null>(null);
  const [llamaProgress, setLlamaProgress] = useState<DownloadProgress | null>(null);
  const [defaultPrompt, setDefaultPrompt] = useState("");
  const [prompt, setPrompt] = useState(s.enhance_system_prompt);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  const refreshLlama = () => api.llamaStatus().then(setLlama);
  useEffect(() => {
    refreshLlama();
    api.defaultSystemPrompt().then(setDefaultPrompt);
  }, []);
  useEvent<DownloadProgress>("download-progress", (p) => {
    if (p.id === "llama-server") setLlamaProgress(p);
  });
  useEvent("llama-installed", () => {
    setLlamaProgress(null);
    refreshLlama();
  });

  const runTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult({ ok: true, text: await api.enhanceTest() });
    } catch (e) {
      setTestResult({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
      refreshLlama();
    }
  };

  const llamaDownloading = llama?.downloading || llamaProgress?.state === "downloading" || llamaProgress?.state === "verifying";

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("enh.title")}</h1>
      </header>
      <p className="page-sub">{t("enh.subtitle")}</p>

      <Card>
        <Row label={t("enh.mode")}>
          <Segmented<EnhanceMode>
            value={s.enhance_mode}
            options={[
              ["off", t("enh.off")],
              ["always", t("enh.always")],
              ["second_shortcut", t("enh.second")],
            ]}
            onChange={(enhance_mode) => update({ enhance_mode })}
          />
        </Row>
        {s.enhance_mode === "second_shortcut" && !s.shortcut_dictate_enhance && (
          <div className="card-inset">
            <div className="notice warning">
              <AlertTriangle size={15} />
              <div>{t("enh.second_missing")}</div>
            </div>
          </div>
        )}
        <Row
          label={t("enh.provider")}
          hint={s.enhance_provider === "local" ? t("enh.local_hint") : s.enhance_provider === "cli" ? t("enh.cli_hint") : t("enh.api_hint")}
        >
          <Segmented<EnhanceProvider>
            value={s.enhance_provider}
            options={[
              ["local", t("enh.local")],
              ["cli", t("enh.cli")],
              ["api", t("enh.api")],
            ]}
            onChange={(enhance_provider) => update({ enhance_provider })}
          />
        </Row>
      </Card>

      {s.enhance_provider === "local" && (
        <Card title={t("enh.local")}>
          <Row
            label={t("enh.runtime")}
            hint={llama?.installed ? t("enh.runtime_ok") : t("enh.runtime_missing", { size: formatBytes(llama?.size ?? 0) })}
          >
            {llama?.installed ? (
              <span className="badge success">
                <Check size={12} /> {t("common.installed")}
              </span>
            ) : llamaDownloading ? (
              <LoaderCircle size={16} className="spin muted" />
            ) : (
              <button type="button" className="btn small primary" onClick={() => api.installLlama().then(refreshLlama)}>
                <Download size={14} /> {t("common.download")}
              </button>
            )}
          </Row>
          {llamaDownloading && llamaProgress && (
            <div className="card-inset">
              <Progress value={llamaProgress.total ? llamaProgress.downloaded / llamaProgress.total : 0} />
            </div>
          )}
          {llamaProgress?.state === "error" && (
            <div className="card-inset">
              <div className="notice danger">{llamaProgress.error}</div>
            </div>
          )}
          {models
            .filter((m) => m.kind === "llm")
            .map((m) => (
              <ModelRow
                key={m.id}
                model={m}
                progress={progress[m.id]}
                selected={s.enhance_local_model === m.id}
                onSelect={() => update({ enhance_local_model: m.id })}
                onChanged={refresh}
              />
            ))}
          <Row label={t("enh.unload")} hint={t("enh.unload_hint")}>
            {llama?.running && (
              <button
                type="button"
                className="btn ghost small"
                onClick={async () => {
                  await api.enhanceStopLocal();
                  refreshLlama();
                }}
              >
                {t("enh.unload_now")}
              </button>
            )}
            <Select
              value={String(s.enhance_unload_after_min)}
              options={[
                ["2", t("adv.minutes", { n: 2 })],
                ["5", t("adv.minutes", { n: 5 })],
                ["15", t("adv.minutes", { n: 15 })],
                ["60", t("adv.minutes", { n: 60 })],
                ["0", t("adv.never")],
              ]}
              onChange={(v) => update({ enhance_unload_after_min: Number(v) })}
            />
          </Row>
        </Card>
      )}

      {s.enhance_provider === "cli" && (
        <Card title={t("enh.cli")}>
          <Row label={t("enh.preset")}>
            <Select
              value={s.enhance_cli_preset}
              options={[
                ["claude", "Claude Code"],
                ["codex", "Codex CLI"],
                ["gemini", "Gemini CLI"],
                ["custom", "Custom"],
              ]}
              onChange={(preset) =>
                update({
                  enhance_cli_preset: preset,
                  ...(CLI_PRESETS[preset] ? { enhance_cli_command: CLI_PRESETS[preset] } : {}),
                })
              }
            />
          </Row>
          <Row label={t("enh.command")} hint={t("enh.command_hint")} stack>
            <input
              className="input wide mono"
              type="text"
              aria-label={t("enh.command")}
              defaultValue={s.enhance_cli_command}
              key={s.enhance_cli_command}
              onBlur={(e) => update({ enhance_cli_command: e.target.value, enhance_cli_preset: "custom" })}
            />
          </Row>
        </Card>
      )}

      {s.enhance_provider === "api" && (
        <Card title={t("enh.api")}>
          <Row label={t("enh.base_url")} stack>
            <input
              className="input wide mono"
              type="text"
              aria-label={t("enh.base_url")}
              defaultValue={s.enhance_api_base_url}
              onBlur={(e) => update({ enhance_api_base_url: e.target.value.trim() })}
            />
          </Row>
          <Row label={t("enh.api_key")} stack>
            <input
              className="input wide mono"
              type="password"
              autoComplete="off"
              aria-label={t("enh.api_key")}
              defaultValue={s.enhance_api_key}
              onBlur={(e) => update({ enhance_api_key: e.target.value.trim() })}
            />
          </Row>
          <Row label={t("enh.api_model")} stack>
            <input
              className="input wide mono"
              type="text"
              aria-label={t("enh.api_model")}
              defaultValue={s.enhance_api_model}
              onBlur={(e) => update({ enhance_api_model: e.target.value.trim() })}
            />
          </Row>
        </Card>
      )}

      <Card
        title={t("enh.prompt")}
        sub={t("enh.prompt_hint")}
        actions={
          <button
            type="button"
            className="btn ghost small"
            onClick={() => {
              setPrompt("");
              update({ enhance_system_prompt: "" });
            }}
          >
            {t("enh.prompt_reset")}
          </button>
        }
      >
        <div className="card-body">
          <textarea
            className="textarea mono"
            aria-label={t("enh.prompt")}
            rows={12}
            value={prompt || defaultPrompt}
            onChange={(e) => setPrompt(e.target.value)}
            onBlur={() => update({ enhance_system_prompt: prompt === defaultPrompt ? "" : prompt })}
          />
        </div>
        <Row label={t("enh.timeout")}>
          <input
            className="input num"
            type="number"
            aria-label={t("enh.timeout")}
            min={5}
            max={300}
            style={{ width: "6rem" }}
            defaultValue={s.enhance_timeout_sec}
            onBlur={(e) => update({ enhance_timeout_sec: Math.max(5, Number(e.target.value) || 30) })}
          />
        </Row>
      </Card>

      <Card>
        <Row label={t("enh.test")}>
          <button type="button" className="btn primary" disabled={testing} aria-busy={testing} onClick={runTest}>
            {testing ? <LoaderCircle size={15} className="spin" /> : <FlaskConical size={15} />}
            {testing ? t("enh.test_running") : t("common.test")}
          </button>
        </Row>
        {testResult && (
          <div className="card-inset">
            <div className={`notice ${testResult.ok ? "success" : "danger"}`}>
              <div className="selectable pre">
                {testResult.ok && <strong>{t("enh.test_ok")} </strong>}
                {testResult.text}
              </div>
            </div>
          </div>
        )}
      </Card>
    </>
  );
}
