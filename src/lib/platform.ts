// Plattform, auf der die Oberfläche läuft. Das Backend setzt sie vor dem Laden
// jedes Fensters (lib.rs: platform_script); ohne Angabe gilt Windows, so laufen
// auch die UI-Tests im Browser.

declare global {
  interface Window {
    __WB_PLATFORM__?: string;
  }
}

export const platform: string = (typeof window !== "undefined" && window.__WB_PLATFORM__) || "windows";
export const isMac = platform === "macos";
