import { useEffect, useState } from "react";
import { FileAudio, FolderOpen, FolderSearch, LoaderCircle, Upload, X } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type FileJob } from "../lib/api";
import { Card, CopyButton, Progress, Row, Segmented, useApp, useEvent } from "../lib/app";
import type { TKey } from "../lib/i18n";

const STATUS_KEY: Record<FileJob["status"], TKey> = {
  queued: "files.queued",
  decoding: "files.decoding",
  transcribing: "files.transcribing",
  done: "files.done",
  error: "files.error",
  cancelled: "files.cancelled",
};

export default function Files() {
  const { t, settings: s, update } = useApp();
  const [jobs, setJobs] = useState<FileJob[]>([]);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    api.filesList().then(setJobs);
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") setDragging(true);
      else if (p.type === "leave") setDragging(false);
      else if (p.type === "drop") {
        setDragging(false);
        if (p.paths.length) api.filesAdd(p.paths);
      }
    });
    return () => {
      unlisten.then((u) => u());
    };
  }, []);

  useEvent<FileJob>("file-job", (job) =>
    setJobs((all) => {
      const i = all.findIndex((j) => j.id === job.id);
      if (i < 0) return [...all, job];
      const next = [...all];
      next[i] = job;
      return next;
    }),
  );
  useEvent("file-jobs-cleared", () => api.filesList().then(setJobs));

  const running = jobs.some((j) => ["queued", "decoding", "transcribing"].includes(j.status));

  return (
    <>
      <header className="page-header">
        <h1 className="page-title">{t("files.title")}</h1>
      </header>
      <p className="page-sub">{t("files.subtitle")}</p>

      <div className={`dropzone ${dragging ? "over" : ""}`}>
        <Upload size={32} />
        <div className="dropzone-title">{t("files.drop")}</div>
        <div className="small muted">{t("files.drop_hint")}</div>
        <div className="hstack wrap">
          <button type="button" className="btn primary" onClick={() => api.filesPick()}>
            <FileAudio size={15} /> {t("files.pick")}
          </button>
          <button type="button" className="btn" onClick={() => api.filesPickFolder()}>
            <FolderSearch size={15} /> {t("files.pick_folder")}
          </button>
        </div>
        <div className="small muted">{t("files.formats")}</div>
      </div>

      <Card>
        <Row label={t("files.output")} hint={s.file_output_dir ?? undefined}>
          <Segmented
            value={s.file_output_dir ? "custom" : "same"}
            options={[
              ["same", t("files.output_same")],
              ["custom", t("files.output_choose")],
            ]}
            onChange={async (v) => {
              if (v === "same") update({ file_output_dir: null });
              else {
                const dir = await open({ directory: true });
                if (typeof dir === "string") update({ file_output_dir: dir });
              }
            }}
          />
        </Row>
      </Card>

      {jobs.length > 0 && (
        <Card
          title={t("files.queue")}
          actions={
            !running || jobs.some((j) => ["done", "error", "cancelled"].includes(j.status)) ? (
              <button type="button" className="btn ghost small" onClick={() => api.filesClear()}>
                {t("files.clear")}
              </button>
            ) : undefined
          }
        >
          {jobs.map((j) => {
            const active = ["queued", "decoding", "transcribing"].includes(j.status);
            return (
              <div className="row top" key={j.id}>
                <div className="row-text">
                  <div className="hstack">
                    <span className="row-label ellipsis">
                      {j.name}
                    </span>
                    <span
                      className={`badge ${j.status === "done" ? "success" : j.status === "error" ? "danger" : active ? "accent" : ""}`}
                    >
                      {active && j.status !== "queued" && <LoaderCircle size={11} className="spin" />}
                      {t(STATUS_KEY[j.status])}
                      {active && j.status !== "queued" && ` ${Math.round(j.progress * 100)} %`}
                    </span>
                  </div>
                  {active && j.status !== "queued" && (
                    <div className="download">
                      <Progress value={j.progress} />
                    </div>
                  )}
                  {j.error && <div className="preview error-text">{j.error}</div>}
                  {j.preview && <div className="preview selectable">{j.preview} …</div>}
                  {j.output && <div className="preview mono">{j.output}</div>}
                </div>
                <div className="row-control">
                  {j.output && (
                    <button type="button" className="btn ghost small" onClick={() => revealItemInDir(j.output!)}>
                      <FolderOpen size={14} /> {t("files.show")}
                    </button>
                  )}
                  {j.preview && <CopyButton text={j.preview} small />}
                  {active && (
                    <button type="button" className="btn ghost icon" aria-label={t("common.cancel")} onClick={() => api.filesCancel(j.id)}>
                      <X size={15} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </Card>
      )}
    </>
  );
}
