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
import { useApp } from "./lib/app";
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
  sepBefore?: boolean;
}

const PAGES: PageDef[] = [
  { id: "home", key: "nav.home", icon: House, comp: Home },
  { id: "dictation", key: "nav.dictation", icon: Mic, comp: Dictation, sepBefore: true },
  { id: "models", key: "nav.models", icon: Boxes, comp: Models },
  { id: "shortcuts", key: "nav.shortcuts", icon: Keyboard, comp: Shortcuts },
  { id: "enhance", key: "nav.enhance", icon: Sparkles, comp: Enhance },
  { id: "edit", key: "nav.edit", icon: PenLine, comp: EditText },
  { id: "words", key: "nav.words", icon: BookText, comp: Words },
  { id: "meetings", key: "nav.meetings", icon: Users, comp: Meetings, sepBefore: true },
  { id: "files", key: "nav.files", icon: FileAudio, comp: Files },
  { id: "history", key: "nav.history", icon: HistoryIcon, comp: History },
  { id: "stats", key: "nav.stats", icon: ChartColumn, comp: Stats },
  { id: "advanced", key: "nav.advanced", icon: SlidersHorizontal, comp: Advanced, sepBefore: true },
  { id: "help", key: "nav.help", icon: CircleHelp, comp: Help },
];

export default function App() {
  const { t, settings } = useApp();
  const [page, setPage] = useState<PageId>("home");
  const [onboarding, setOnboarding] = useState(!settings.onboarding_done);
  const [meetingActive, setMeetingActive] = useState(false);

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
      <nav className="sidebar">
        <div className="brand">
          <Bird size={36} />
          <div>
            <div className="brand-name">{t("app.name")}</div>
            <div className="brand-sub">Windows · lokal</div>
          </div>
        </div>
        {PAGES.map((p) => {
          const Icon = p.icon;
          return (
            <div key={p.id} style={{ display: "contents" }}>
              {p.sepBefore && <div className="nav-sep" />}
              <button
                className="nav-item"
                aria-current={p.id === page ? "page" : undefined}
                onClick={() => setPage(p.id)}
              >
                <Icon size={17} />
                {t(p.key)}
                {p.id === "meetings" && meetingActive && <span className="nav-badge" />}
              </button>
            </div>
          );
        })}
      </nav>
      <main className="content">
        <div className="page" key={current.id}>
          <Comp navigate={setPage} />
        </div>
      </main>
    </div>
  );
}
