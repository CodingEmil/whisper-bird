import { useEffect, useState, type ComponentType } from "react";
import {
  BookText,
  Boxes,
  ChartColumn,
  CircleHelp,
  FileAudio,
  History as HistoryIcon,
  House,
  Keyboard,
  Mic,
  PenLine,
  SlidersHorizontal,
  Sparkles,
  Users,
  type LucideIcon,
} from "lucide-react";
import { api, on } from "./lib/api";
import { useApp, useWindowShortcuts } from "./lib/app";
import type { TKey } from "./lib/i18n";
import Home from "./pages/Home";
import Dictation from "./pages/Dictation";
import Models from "./pages/Models";
import Shortcuts from "./pages/Shortcuts";
import Enhance from "./pages/Enhance";
import EditText from "./pages/EditText";
import Words from "./pages/Words";
import History from "./pages/History";
import Stats from "./pages/Stats";
import Meetings from "./pages/Meetings";
import Files from "./pages/Files";
import Advanced from "./pages/Advanced";
import Help from "./pages/Help";
import Onboarding from "./pages/Onboarding";
import { Bird } from "./lib/bird";

export type PageId =
  | "home"
  | "dictation"
  | "models"
  | "shortcuts"
  | "enhance"
  | "edit"
  | "words"
  | "history"
  | "stats"
  | "meetings"
  | "files"
  | "advanced"
  | "help";

interface PageDef {
  id: PageId;
  key: TKey;
  icon: LucideIcon;
  comp: ComponentType<{ navigate: (p: PageId) => void }>;
}

interface NavGroup {
  heading?: TKey;
  /** Steht am Fuß der Seitenleiste. */
  foot?: boolean;
  pages: PageDef[];
}

const NAV: NavGroup[] = [
  { pages: [{ id: "home", key: "nav.home", icon: House, comp: Home }] },
  {
    heading: "nav.group_dictate",
    pages: [
      { id: "dictation", key: "nav.dictation", icon: Mic, comp: Dictation },
      { id: "models", key: "nav.models", icon: Boxes, comp: Models },
      { id: "shortcuts", key: "nav.shortcuts", icon: Keyboard, comp: Shortcuts },
      { id: "enhance", key: "nav.enhance", icon: Sparkles, comp: Enhance },
      { id: "edit", key: "nav.edit", icon: PenLine, comp: EditText },
      { id: "words", key: "nav.words", icon: BookText, comp: Words },
    ],
  },
  {
    heading: "nav.group_recordings",
    pages: [
      { id: "meetings", key: "nav.meetings", icon: Users, comp: Meetings },
      { id: "files", key: "nav.files", icon: FileAudio, comp: Files },
      { id: "history", key: "nav.history", icon: HistoryIcon, comp: History },
      { id: "stats", key: "nav.stats", icon: ChartColumn, comp: Stats },
    ],
  },
  {
    foot: true,
    pages: [
      { id: "advanced", key: "nav.advanced", icon: SlidersHorizontal, comp: Advanced },
      { id: "help", key: "nav.help", icon: CircleHelp, comp: Help },
    ],
  },
];

const PAGES = NAV.flatMap((g) => g.pages);

export default function App() {
  const { t, settings } = useApp();
  const [page, setPage] = useState<PageId>("home");
  const [onboarding, setOnboarding] = useState(!settings.onboarding_done);
  const [meetingActive, setMeetingActive] = useState(false);
  useWindowShortcuts();

  useEffect(() => {
    document.documentElement.lang = settings.ui_language;
  }, [settings.ui_language]);

  useEffect(() => {
    const off1 = on<string>("navigate", (p) => {
      if (p === "onboarding") setOnboarding(true);
      else if (PAGES.some((x) => x.id === p)) setPage(p as PageId);
    });
    api.meetingActive().then((id) => setMeetingActive(!!id));
    const off2 = on<{ status: string }>("meeting-update", (m) => setMeetingActive(m.status === "recording"));
    return () => {
      off1();
      off2();
    };
  }, []);

  if (onboarding) {
    return <Onboarding onDone={() => setOnboarding(false)} />;
  }

  const current = PAGES.find((p) => p.id === page) ?? PAGES[0];
  const Comp = current.comp;
  return (
    <div className="layout">
      <nav className="sidebar" aria-label={t("app.name")}>
        <div className="brand">
          <Bird size={36} />
          <div>
            <div className="brand-name">{t("app.name")}</div>
            <div className="brand-sub">lokal auf Windows</div>
          </div>
        </div>
        {NAV.map((g, i) => (
          <div
            key={i}
            className={`nav-group${g.foot ? " foot" : ""}`}
            role={g.heading ? "group" : undefined}
            aria-labelledby={g.heading ? `nav-heading-${i}` : undefined}
          >
            {g.heading && (
              <div className="nav-heading" id={`nav-heading-${i}`}>
                {t(g.heading)}
              </div>
            )}
            {g.pages.map((p) => {
              const Icon = p.icon;
              return (
                <button
                  key={p.id}
                  type="button"
                  className="nav-item"
                  aria-current={p.id === page ? "page" : undefined}
                  title={t(p.key)}
                  onClick={() => setPage(p.id)}
                >
                  <Icon size={18} />
                  <span className="nav-label">{t(p.key)}</span>
                  {p.id === "meetings" && meetingActive && <span className="nav-badge">{t("meet.recording")}</span>}
                </button>
              );
            })}
          </div>
        ))}
      </nav>
      <main className="content">
        <div className="page" key={current.id}>
          <Comp navigate={setPage} />
        </div>
      </main>
    </div>
  );
}
