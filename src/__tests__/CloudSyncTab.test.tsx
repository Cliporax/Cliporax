import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import CloudSyncTab from "../components/Settings/CloudSyncTab";
import { sync, events, type SyncConflict, type SyncProfile, type SyncRunReport, type SyncCompletedPayload } from "../lib/tauri-api";
import en from "../locales/en.json";

vi.mock("../lib/tauri-api", () => ({
  sync: { profileList: vi.fn(), profileGet: vi.fn(), getTabOptions: vi.fn(), getPluginOptions: vi.fn(), getStatus: vi.fn(), getLastReport: vi.fn(), getConflicts: vi.fn(), getLogEntries: vi.fn(), resolveConflict: vi.fn(), profileUpdate: vi.fn(), runNow: vi.fn() },
  events: { onSyncCompleted: vi.fn() },
}));
const i18n = createInstance();
void i18n.init({ lng: "en", resources: { en: { translation: en } }, interpolation: { escapeValue: false } });
let pending: SyncConflict[];
let onCompleted: (payload: SyncCompletedPayload | null) => void;
const profile: SyncProfile = {
  id: "profile", name: "My sync", provider: "webdav", remote_root: "https://example.test/sync",
  sync_tabs: { mode: "all", selected_tab_ids: [] }, sync_plugins: { mode: "selected", selected_plugin_ids: [], include_plugin_bundles: false, include_granted_permissions: false },
  encryption: { enabled: false, algorithm: "xchacha20poly1305", kdf: "argon2id" }, credential_refs: {}, created_at: null, updated_at: null, schedule: { manual: true, startup_delay_seconds: 15, local_change_debounce_seconds: 30, retry_backoff_seconds: [30, 120], pause_on_metered_network: false, sync_on_startup: true, sync_on_local_change: true, interval_minutes: 5, paused: false },
};
const report: SyncRunReport = {
  profile_id: "profile", run_id: "run", status: "partial_success", started_at: "2026-10-10T01:00:00Z", completed_at: "2026-10-10T01:00:05Z",
  items_uploaded: 10, items_downloaded: 20, items_deleted: 0, conflicts_found: 1, errors: ["Unreadable shard retained"], warnings: ["Snapshot hash mismatch recovered"],
};
function conflict(type = "clipboard_item"): SyncConflict {
  return { id: 1, entity_type: type, entity_key: "shared", local_payload: JSON.stringify(type === "plugin_data" ? { value: { tasks: ["local todo"] } } : { content: "Local choice", is_pinned: true }),
    remote_payload: JSON.stringify(type === "plugin_data" ? { value: { tasks: ["remote todo"] } } : { content: "Remote choice", is_pinned: false }), reason: "Concurrent edits", status: "pending", resolution: null, created_at: "2026-10-10T01:00:00Z", resolved_at: null };
}
function mount() { return render(<I18nextProvider i18n={i18n}><CloudSyncTab isDark /></I18nextProvider>); }
async function openConflicts() { fireEvent.click((await screen.findByText("Conflicts")).closest("button")!); await screen.findByRole("button", { name: "Prefer local" }); }

beforeEach(() => {
  vi.clearAllMocks(); pending = [conflict()];
  vi.mocked(sync.profileList).mockResolvedValue([{ id: "profile" }] as Awaited<ReturnType<typeof sync.profileList>>);
  vi.mocked(sync.profileGet).mockResolvedValue(profile);
  vi.mocked(sync.getTabOptions).mockResolvedValue([]); vi.mocked(sync.getPluginOptions).mockResolvedValue([]);
  vi.mocked(sync.getStatus).mockResolvedValue(null as unknown as Awaited<ReturnType<typeof sync.getStatus>>);
  vi.mocked(sync.getLastReport).mockResolvedValue(report); vi.mocked(sync.getLogEntries).mockResolvedValue([]);
  vi.mocked(sync.getConflicts).mockImplementation(async () => pending);
  vi.mocked(sync.resolveConflict).mockImplementation(async () => { pending = []; return { report: { ...report, status: "completed", errors: [], conflicts_found: 0 }, sync_error: null }; });
  vi.mocked(sync.profileUpdate).mockResolvedValue();
  vi.mocked(sync.runNow).mockImplementation(async () => { pending = []; return { ...report, status: "completed", errors: [], conflicts_found: 0 }; });
  vi.mocked(events.onSyncCompleted).mockImplementation(async (callback) => { onCompleted = callback; return () => {}; });
});
afterEach(cleanup);

describe("Cloud sync recovery and user conflicts", () => {
  it("shows recovery and partial progress and navigates to conflicts", async () => {
    mount(); fireEvent.click((await screen.findByText("Status")).closest("button")!);
    await screen.findByText("An outdated snapshot was recovered. Sync continued normally.");
    expect(screen.getByText("Available items were synced. Some data could not be read and is kept for retry.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review 1 pending conflicts. Other items continue syncing." }));
    expect(screen.getByText(/Local choice/)).toBeTruthy(); expect(screen.getByText(/Remote choice/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Prefer remote" }));
    await screen.findByText("No pending conflicts");
    expect(sync.resolveConflict).toHaveBeenCalledWith("profile", 1, "use_remote");
  });
  it("blocks repeated resolution requests and retains the choice after a failed request", async () => {
    let reject!: (error: Error) => void;
    vi.mocked(sync.resolveConflict).mockImplementation(() => new Promise((_, fail) => { reject = fail; }));
    mount(); await openConflicts(); const button = screen.getByRole("button", { name: "Prefer local" });
    fireEvent.click(button); fireEvent.click(button);
    expect((button as HTMLButtonElement).disabled).toBe(true); expect(sync.resolveConflict).toHaveBeenCalledTimes(1);
    reject(new Error("Offline"));
    await screen.findByText("Resolve conflict failed: Error: Offline");
    expect((screen.getByRole("button", { name: "Prefer local" }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByText(/Local choice/)).toBeTruthy();
  });
  it("offers local and remote plugin versions and refreshes after automatic sync", async () => {
    pending = [conflict("plugin_data")]; mount(); await openConflicts();
    expect(screen.getByText(/local todo/)).toBeTruthy(); expect(screen.getByText(/remote todo/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Keep both" })).toBeNull();
    await waitFor(() => expect(events.onSyncCompleted).toHaveBeenCalled());
    pending = []; onCompleted({ profileId: "profile", report });
    await screen.findByText("No pending conflicts");
  });
  it("saves an automatic remote default and immediately processes existing conflicts", async () => {
    mount(); await openConflicts();
    fireEvent.click(screen.getByRole("combobox", { name: "Default conflict handling" }));
    fireEvent.click(screen.getByRole("option", { name: "Automatically prefer remote" }));
    fireEvent.click(screen.getByRole("button", { name: "Save and apply" }));
    await screen.findByText("Default saved. Conflicts processed and synced to the remote.");
    expect(sync.profileUpdate).toHaveBeenCalledWith(expect.objectContaining({ id: "profile", conflict_policy: "prefer_remote", remote_root: profile.remote_root }));
    expect(sync.runNow).toHaveBeenCalledWith("profile");
    expect(screen.getByText("No pending conflicts")).toBeTruthy();
  });
  it("preserves a manual default without automatically resolving pending choices", async () => {
    mount(); await openConflicts();
    expect(screen.getByRole("combobox", { name: "Default conflict handling" }).textContent).toContain("Ask me each time");
    fireEvent.click(screen.getByRole("button", { name: "Save and apply" }));
    await screen.findByText("Default saved. Future conflicts will ask for your choice.");
    expect(sync.runNow).not.toHaveBeenCalled();
    expect(screen.getByText(/Local choice/)).toBeTruthy();
  });
  it("shows a saved choice awaiting publication when remote upload fails", async () => {
    vi.mocked(sync.resolveConflict).mockImplementation(async () => { pending = []; return { report: { ...report, errors: ["Offline"] }, sync_error: null }; });
    mount(); await openConflicts();
    fireEvent.click(screen.getByRole("button", { name: "Prefer local" }));
    await screen.findByText("Choice saved. Remote update failed; it will retry on the next sync: Offline");
    expect(screen.getByText("No pending conflicts")).toBeTruthy();
  });
  it("retains the saved automatic default when its immediate sync fails", async () => {
    vi.mocked(sync.runNow).mockRejectedValue(new Error("Offline"));
    mount(); await openConflicts();
    fireEvent.click(screen.getByRole("combobox", { name: "Default conflict handling" }));
    fireEvent.click(screen.getByRole("option", { name: "Automatically prefer local" }));
    fireEvent.click(screen.getByRole("button", { name: "Save and apply" }));
    await screen.findByText("Default saved. Sync failed; retry sync to apply it: Error: Offline");
    expect(sync.profileUpdate).toHaveBeenCalledWith(expect.objectContaining({ conflict_policy: "prefer_local" }));
    expect(screen.getByText(/Local choice/)).toBeTruthy();
  });

});
