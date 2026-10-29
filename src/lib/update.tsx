import { useEffect, useId, useRef, useState } from "react";
import { api, on, type DownloadProgress, type UpdateStatus } from "./api";
import { useApp } from "./app";
import { Bird } from "./bird";

/** Download-ID des Installers, wie im Backend (updates::DOWNLOAD_ID). */
const DOWNLOAD_ID = "app-update";

/** So lange darf die erste Update-Suche dauern, die die Neuerungen mitbringt. */
const NOTES_WAIT_MS = 2500;

/** Stand der Update-Suche und, während der Installer lädt, dessen Fortschritt. */
export function useUpdate() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  useEffect(() => {
    api.updateStatus().then(setStatus);
    const off1 = on<UpdateStatus>("update-status", setStatus);
    const off2 = on<DownloadProgress>("download-progress", (p) => {
      if (p.id === DOWNLOAD_ID) setProgress(p);
    });
    return () => {
      off1();
      off2();
    };
  }, []);
  return { status, progress };
}

/** Einmal nach einem Update: welche Version jetzt läuft und, falls bekannt, was neu ist. */
export function UpdateNotice({ status }: { status: UpdateStatus | null }) {
  const { t, settings } = useApp();
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const [waited, setWaited] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const pending = !!status?.just_updated && !dismissed;

  useEffect(() => {
    if (!pending) return;
    const timer = window.setTimeout(() => setWaited(true), NOTES_WAIT_MS);
    return () => window.clearTimeout(timer);
  }, [pending]);

  // Mit den Neuerungen öffnen, sobald die Suche durch ist – oder es keine gibt.
  const searched = !settings.check_updates || status?.checked_at != null || status?.error != null;
  const open = pending && (searched || waited);

  useEffect(() => {
    const d = dialog.current;
    if (open && d && !d.open) d.showModal();
  }, [open]);

  if (!open || !status) return null;

  const close = () => {
    setDismissed(true);
    void api.updateNoticeSeen();
  };

  return (
    <dialog
      ref={dialog}
      className="confirm update-notice"
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
    >
      <div className="confirm-inner">
        <Bird size={40} />
        <div className="confirm-text">
          <h2 className="confirm-title" id={titleId}>
            {t("update.done_title", { version: status.current })}
          </h2>
          <p className="confirm-body">{t(status.notes.length > 0 ? "update.done_notes" : "update.done_body")}</p>
          {status.notes.length > 0 && (
            <ul className="update-notes">
              {status.notes.map((note, i) => (
                <li key={i}>{note}</li>
              ))}
            </ul>
          )}
        </div>
        <div className="confirm-actions">
          <button type="button" className="btn primary" autoFocus onClick={close}>
            {t("update.done_ok")}
          </button>
        </div>
      </div>
    </dialog>
  );
}
