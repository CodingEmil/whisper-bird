import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Activation = "toggle" | "push_to_talk" | "hybrid";
export type PasteMethod = "clipboard" | "type";
export type EnhanceMode = "off" | "always" | "second_shortcut";
export type EnhanceProvider = "local" | "cli" | "api";
export type OverlayPosition = "top_center" | "bottom_center";
export type MeetingSource = "microphone" | "system" | "both";
export type Accelerator = "auto" | "cpu" | "gpu";

export interface Snippet {
  trigger: string;
  text: string;
}

export interface EditAction {
  id: string;
  label: string;
  instruction: string;
}

export interface Settings {
  ui_language: string;
  onboarding_done: boolean;
  asr_model: string;
  language: string;
  activation: Activation;
  shortcut_dictate: string;
  shortcut_dictate_enhance: string;
  shortcut_edit: string;
  shortcut_meeting: string;
  microphone: string | null;
  accelerator: Accelerator;
  unload_after_min: number;
  show_floating_icon: boolean;
  floating_pos: [number, number] | null;
  overlay_position: OverlayPosition;
  sounds: boolean;
  sound_volume: number;
  audio_ducking: boolean;
  ducking_level: number;
  paste_method: PasteMethod;
  preserve_clipboard: boolean;
  trailing_space: boolean;
  auto_enter_apps: string[];
  custom_words: string[];
  snippets: Snippet[];
  remove_fillers: boolean;
  enhance_mode: EnhanceMode;
  enhance_provider: EnhanceProvider;
  enhance_local_model: string;
  enhance_keep_loaded: boolean;
  enhance_cli_preset: string;
  enhance_cli_command: string;
  enhance_api_base_url: string;
  enhance_api_key: string;
  enhance_api_model: string;
  enhance_system_prompt: string;
  enhance_timeout_sec: number;
  edit_actions: EditAction[];
  translate_target: string;
  history_limit: number;
  meeting_source: MeetingSource;
  meeting_auto_notes: boolean;
  file_output_dir: string | null;
  autostart: boolean;
  start_hidden: boolean;
  debug_logging: boolean;
}

export interface ShortcutError {
  field: string;
  message: string;
}

export type ModelKind = "asr" | "vad" | "diarize" | "llm";

export interface ModelStatus {
  id: string;
  name: string;
  description_de: string;
  description_en: string;
  kind: ModelKind;
  family: string;
  size: number;
  languages: string[];
  speed: number;
  accuracy: number;
  recommended: boolean;
  installed: boolean;
  downloading: boolean;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number;
  state: "downloading" | "verifying" | "done" | "error" | "cancelled";
  error: string | null;
}

export interface LlamaStatus {
  installed: boolean;
  running: boolean;
  downloading: boolean;
  size: number;
}

export interface MicDevice {
  id: string;
  name: string;
  is_default: boolean;
}

export interface HistoryEntry {
  id: number;
  created_at: number;
  text: string;
  raw_text: string;
  source: "dictation" | "enhanced" | "edit" | "file" | "meeting";
  app: string;
  language: string | null;
  duration_ms: number;
  words: number;
  model: string;
}

export interface HistoryQuery {
  search?: string;
  from?: number | null;
  to?: number | null;
  oldest_first?: boolean;
  source?: string | null;
  limit?: number;
  offset?: number;
}

export interface Stats {
  count: number;
  duration_ms: number;
  words: number;
  enhanced: number;
  edits: number;
  files: number;
  meetings: number;
  languages: { language: string; count: number; words: number }[];
  days: { day: string; count: number; words: number }[];
  first_use: number | null;
}

export interface AppInfo {
  version: string;
  data_dir: string;
  models_dir: string;
  log_dir: string;
  devices: string[];
  asr_loaded: string | null;
}

export interface FileJob {
  id: number;
  path: string;
  name: string;
  status: "queued" | "decoding" | "transcribing" | "done" | "error" | "cancelled";
  progress: number;
  output: string | null;
  error: string | null;
  preview: string | null;
}

export interface MeetingSegment {
  start_ms: number;
  end_ms: number;
  speaker: number;
  text: string;
}

export interface Meeting {
  id: string;
  title: string;
  created_at: number;
  duration_ms: number;
  source: MeetingSource;
  status: "recording" | "processing" | "done" | "error";
  segments: MeetingSegment[];
  speakers: string[];
  notes: string | null;
  error: string | null;
  diarized: boolean;
}

export interface MeetingSummary {
  id: string;
  title: string;
  created_at: number;
  duration_ms: number;
  status: Meeting["status"];
  speakers: number;
  preview: string;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: Partial<Settings>) =>
    invoke<{ settings: Settings; shortcut_errors: ShortcutError[] }>("set_settings", { patch }),
  defaultSystemPrompt: () => invoke<string>("default_system_prompt"),
  defaultEditActions: () => invoke<EditAction[]>("default_edit_actions"),
  appInfo: () => invoke<AppInfo>("app_info"),
  listModels: () => invoke<ModelStatus[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  llamaStatus: () => invoke<LlamaStatus>("llama_status"),
  installLlama: () => invoke<void>("install_llama"),
  listMicrophones: () => invoke<MicDevice[]>("list_microphones"),
  playTestSound: () => invoke<void>("play_test_sound"),
  startShortcutCapture: (session: string) => invoke<void>("start_shortcut_capture", { session }),
  stopShortcutCapture: (session: string) => invoke<void>("stop_shortcut_capture", { session }),
  dictationToggle: (enhance = false) => invoke<void>("dictation_toggle", { enhance }),
  dictationCancel: () => invoke<void>("dictation_cancel"),
  overlayContextMenu: () => invoke<void>("overlay_context_menu"),
  showSettings: (page?: string) => invoke<void>("show_settings", { page: page ?? null }),
  historyQuery: (query: HistoryQuery) => invoke<HistoryEntry[]>("history_query", { query }),
  historyDelete: (id: number) => invoke<void>("history_delete", { id }),
  historyClear: () => invoke<void>("history_clear"),
  stats: () => invoke<Stats>("stats"),
  resetStats: () => invoke<void>("reset_stats"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  enhanceTest: () => invoke<string>("enhance_test"),
  enhanceStopLocal: () => invoke<void>("enhance_stop_local"),
  editmenuRun: (instruction: string) => invoke<string>("editmenu_run", { instruction }),
  editmenuApply: (text: string) => invoke<void>("editmenu_apply", { text }),
  editmenuCopy: (text: string) => invoke<void>("editmenu_copy", { text }),
  editmenuClose: () => invoke<void>("editmenu_close"),
  editmenuVoice: (start: boolean) => invoke<void>("editmenu_voice", { start }),
  filesPick: () => invoke<void>("files_pick"),
  filesPickFolder: () => invoke<void>("files_pick_folder"),
  filesAdd: (paths: string[]) => invoke<void>("files_add", { paths }),
  filesList: () => invoke<FileJob[]>("files_list"),
  filesCancel: (id: number) => invoke<void>("files_cancel", { id }),
  filesClear: () => invoke<void>("files_clear"),
  meetingStart: () => invoke<string>("meeting_start"),
  meetingStop: () => invoke<void>("meeting_stop"),
  meetingActive: () => invoke<string | null>("meeting_active"),
  meetingList: () => invoke<MeetingSummary[]>("meeting_list"),
  meetingGet: (id: string) => invoke<Meeting>("meeting_get", { id }),
  meetingDelete: (id: string) => invoke<void>("meeting_delete", { id }),
  meetingUpdate: (id: string, title?: string, speakers?: string[]) =>
    invoke<Meeting>("meeting_update", { id, title: title ?? null, speakers: speakers ?? null }),
  meetingNotes: (id: string) => invoke<Meeting>("meeting_notes", { id }),
  meetingTranscript: (id: string) => invoke<string>("meeting_transcript", { id }),
  saveTextFile: (path: string, content: string) => invoke<void>("save_text_file", { path, content }),
  finishOnboarding: () => invoke<void>("finish_onboarding"),
  openDataDir: (which: "data" | "logs" | "models" | "meetings") => invoke<void>("open_data_dir", { which }),
};

export function on<T>(event: string, handler: (payload: T) => void): () => void {
  let unlisten: UnlistenFn | null = null;
  let disposed = false;
  listen<T>(event, (e) => handler(e.payload)).then((u) => {
    if (disposed) u();
    else unlisten = u;
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
}

export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${Math.round(n / 1e6)} MB`;
  return `${Math.round(n / 1e3)} KB`;
}

export function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  return `${m}:${String(sec).padStart(2, "0")}`;
}

/** Zeigt handy-keys-Kombis mit Windows-Namen an. */
export function formatShortcut(s: string, lang: string): string {
  if (!s) return "";
  const de = lang === "de";
  const map: Record<string, string> = {
    Ctrl: de ? "Strg" : "Ctrl",
    CtrlLeft: de ? "Strg links" : "Left Ctrl",
    CtrlRight: de ? "Strg rechts" : "Right Ctrl",
    Opt: "Alt",
    OptLeft: "Alt",
    OptRight: "AltGr",
    Shift: de ? "Umschalt" : "Shift",
    ShiftLeft: de ? "Umschalt links" : "Left Shift",
    ShiftRight: de ? "Umschalt rechts" : "Right Shift",
    Cmd: "Win",
    CmdLeft: "Win",
    CmdRight: de ? "Win rechts" : "Right Win",
    Space: de ? "Leertaste" : "Space",
    Escape: "Esc",
    Return: "Enter",
  };
  return s
    .split("+")
    .map((p) => map[p] ?? p)
    .join(" + ");
}
