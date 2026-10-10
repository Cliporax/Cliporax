import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import AppUpdateCard from "../components/Settings/AppUpdateCard";
import { appUpdate, type AppUpdateStatus } from "../lib/tauri-api";
import en from "../locales/en.json";

vi.mock("../lib/tauri-api", () => ({ appUpdate: { status: vi.fn(), check: vi.fn(), download: vi.fn(), install: vi.fn() } }));
const i18n = createInstance();
void i18n.init({ lng: "en", resources: { en: { translation: en } }, interpolation: { escapeValue: false } });

const idle: AppUpdateStatus = {
  phase: "idle", version: null, fileName: null, downloaded: 0, total: 0,
  lastChecked: null, error: null, manualInstall: false, installerOpened: false, packagePath: null,
};
const available: AppUpdateStatus = { ...idle, phase: "available", version: "1.3.0", fileName: "Cliporax_1.3.0_amd64.deb", total: 100 };
const ready: AppUpdateStatus = { ...available, phase: "ready", downloaded: 100, packagePath: "/cache/updates/Cliporax_1.3.0_amd64.deb" };
let backend: AppUpdateStatus;

function mount() {
  return render(<I18nextProvider i18n={i18n}><AppUpdateCard isDark /></I18nextProvider>);
}

beforeEach(() => {
  vi.clearAllMocks();
  backend = { ...idle };
  vi.mocked(appUpdate.status).mockImplementation(async () => backend);
  vi.mocked(appUpdate.check).mockImplementation(async () => (backend = { ...available }));
  vi.mocked(appUpdate.download).mockImplementation(async () => (backend = { ...ready }));
  vi.mocked(appUpdate.install).mockImplementation(async () => (backend = { ...ready, installerOpened: true }));
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };
});
afterEach(cleanup);

describe("About application updates", () => {
  it("checks, downloads, and opens the installer only after confirmation", async () => {
    mount();
    const check = screen.getByRole("button", { name: "Check now" });
    await waitFor(() => expect((check as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(check);
    fireEvent.click(await screen.findByRole("button", { name: "Download update" }));
    const dialog = await screen.findByRole("dialog");
    expect(appUpdate.install).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Install now" }));
    await waitFor(() => expect(appUpdate.install).toHaveBeenCalledTimes(1));
    await screen.findByText("Opened. Follow the installation steps, then restart Cliporax.");
    expect(appUpdate.download).toHaveBeenCalledTimes(1);
  });

  it("retains a deferred download for installation without downloading again", async () => {
    backend = { ...ready };
    mount();
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(appUpdate.install).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Install now" }));
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Install now" }));
    await waitFor(() => expect(appUpdate.install).toHaveBeenCalledTimes(1));
    expect(appUpdate.download).not.toHaveBeenCalled();
  });

  it("shows a download failure and supports retry", async () => {
    backend = { ...available };
    vi.mocked(appUpdate.download).mockImplementationOnce(async () => (backend = { ...available, error: "Connection interrupted" }));
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Download update" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Connection interrupted");
    fireEvent.click(screen.getByRole("button", { name: "Download update" }));
    await screen.findByRole("dialog");
    expect(appUpdate.download).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("recovers from an IPC rejection without leaving the button disabled", async () => {
    vi.mocked(appUpdate.check).mockRejectedValueOnce(new Error("Offline"));
    mount();
    const check = screen.getByRole("button", { name: "Check now" });
    await waitFor(() => expect((check as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(check);
    expect((await screen.findByRole("alert")).textContent).toContain("Offline");
    expect((check as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(check);
    await screen.findByRole("button", { name: "Download update" });
  });

  it("restores progress and prompts when a background download finishes", async () => {
    backend = { ...available, phase: "downloading", downloaded: 42 };
    const first = mount();
    expect((await screen.findByRole("progressbar")).getAttribute("value")).toBe("42");
    expect((screen.getByRole("button", { name: "Please wait…" }) as HTMLButtonElement).disabled).toBe(true);
    first.unmount();
    backend = { ...ready };
    mount();
    await screen.findByRole("dialog");
    expect(appUpdate.download).not.toHaveBeenCalled();
  });

  it("disables repeated actions during a pending download and ignores old polls", async () => {
    backend = { ...available };
    let finish!: (value: AppUpdateStatus) => void;
    vi.mocked(appUpdate.download).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Download update" }));
    const wait = screen.getByRole("button", { name: "Please wait…" }) as HTMLButtonElement;
    expect(wait.disabled).toBe(true);
    fireEvent.click(wait);
    expect(appUpdate.download).toHaveBeenCalledTimes(1);
    await act(async () => { backend = { ...ready }; finish(backend); });
    await screen.findByRole("dialog");
  });

  it("offers a folder instead of an installer for portable packages", async () => {
    backend = { ...ready, manualInstall: true };
    mount();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Keep your existing data folder/)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Open download folder" }));
    await waitFor(() => expect(appUpdate.install).toHaveBeenCalledTimes(1));
  });

  it("shows when the installed version is current", async () => {
    backend = { ...idle, phase: "current", lastChecked: "2026-10-10T00:00:00Z" };
    mount();
    await screen.findByText("You’re using the latest version");
    expect(screen.queryByRole("button", { name: "Download update" })).toBeNull();
  });
});
