// Freigaben unter macOS (Bedienungshilfen, Mikrofon): Status abfragen und anfordern.
// Unter Windows gibt es nichts freizugeben; dort bleibt alles still.

import { useEffect, useState } from "react";
import { Check } from "lucide-react";
import { api, on, type PermissionKind, type Permissions } from "./api";
import { Card, Row, useApp } from "./app";
import { isMac } from "./platform";

/** Aktueller Stand; wird laufend nachgefragt, weil die Freigabe in den Systemeinstellungen passiert. */
export function usePermissions(): Permissions | null {
  const [perms, setPerms] = useState<Permissions | null>(null);
  useEffect(() => {
    if (!isMac) return;
    let alive = true;
    const load = () =>
      api
        .permissionsStatus()
        .then((p) => alive && setPerms(p))
        .catch(() => {});
    load();
    const timer = window.setInterval(load, 2000);
    const off = on("permissions-changed", load);
    return () => {
      alive = false;
      window.clearInterval(timer);
      off();
    };
  }, []);
  return perms;
}

function Action({ kind, granted, denied }: { kind: PermissionKind; granted: boolean; denied: boolean }) {
  const { t } = useApp();
  if (granted) {
    return (
      <span className="badge success">
        <Check size={11} /> {t("perm.granted")}
      </span>
    );
  }
  if (denied) {
    return (
      <button type="button" className="btn small" onClick={() => api.permissionOpenSettings(kind)}>
        {t("perm.open_settings")}
      </button>
    );
  }
  return (
    <button type="button" className="btn small primary" onClick={() => api.permissionRequest(kind)}>
      {t("perm.grant")}
    </button>
  );
}

export function PermissionList({ perms }: { perms: Permissions }) {
  const { t } = useApp();
  return (
    <Card>
      <Row label={t("perm.accessibility")} hint={t("perm.accessibility_hint")}>
        <Action kind="accessibility" granted={perms.accessibility} denied={false} />
      </Row>
      <Row label={t("perm.microphone")} hint={t("perm.microphone_hint")}>
        <Action kind="microphone" granted={perms.microphone === "granted"} denied={perms.microphone === "denied"} />
      </Row>
    </Card>
  );
}
