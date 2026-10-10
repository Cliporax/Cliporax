import { expect, test } from "./fixtures/tauriMock";

test("About downloads in-app, restores progress, and requires installation confirmation", async ({ page, mockTauri }) => {
  await mockTauri();
  await page.goto("/settings");
  await expect(page.getByTestId("settings-panel")).toBeVisible();
  await page.evaluate(() => {
    const win = window as any;
    const original = win.__TAURI_INTERNALS__.invoke;
    const update = {
      phase: "idle", version: null, fileName: null, downloaded: 0, total: 0,
      lastChecked: null, error: null, manualInstall: false, installerOpened: false, packagePath: null,
    };
    win.__appUpdateFixture = update;
    win.__TAURI_INTERNALS__.invoke = async (cmd: string, args: unknown) => {
      if (!cmd.endsWith("app_update") && cmd !== "get_app_update_status") return original(cmd, args);
      win.__cliporaxTauriCalls.push({ cmd, args });
      if (cmd === "check_app_update") Object.assign(update, { phase: "available", version: "1.3.0", fileName: "Cliporax_1.3.0_amd64.deb", total: 100 });
      if (cmd === "download_app_update") {
        Object.assign(update, { phase: "downloading", downloaded: 42 });
        await new Promise<void>(resolve => { win.__finishAppDownload = () => {
          Object.assign(update, { phase: "ready", downloaded: 100, packagePath: "/cache/updates/Cliporax_1.3.0_amd64.deb" });
          resolve();
        }; });
      }
      if (cmd === "install_app_update") update.installerOpened = true;
      return { ...update };
    };
  });

  await page.getByRole("button", { name: "About", exact: true }).click();
  await page.getByRole("button", { name: "Check now", exact: true }).click();
  await page.getByRole("button", { name: "Download update", exact: true }).click();
  await expect(page.getByRole("progressbar")).toHaveAttribute("value", "42");
  await page.getByRole("button", { name: "General", exact: true }).click();
  await page.getByRole("button", { name: "About", exact: true }).click();
  await expect(page.getByRole("progressbar")).toHaveAttribute("value", "42");
  await page.evaluate(() => (window as any).__finishAppDownload());
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Later", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await page.getByRole("button", { name: "Install now", exact: true }).click();
  await expect(dialog).toBeVisible();
  await page.setViewportSize({ width: 480, height: 640 });
  await expect(dialog).toBeInViewport();
  const bounds = await dialog.boundingBox();
  expect(bounds!.width).toBeLessThanOrEqual(480);
  const before = await page.evaluate(() => (window as any).__cliporaxTauriCalls);
  expect(before.filter((call: { cmd: string }) => call.cmd === "install_app_update")).toHaveLength(0);
  expect(before.filter((call: { cmd: string }) => call.cmd === "plugin:shell|open")).toHaveLength(0);
  await page.screenshot({ path: "test-results/app-update-dialog.png" });
  await dialog.getByRole("button", { name: "Install now", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("Follow the installation steps");
  const calls = await page.evaluate(() => (window as any).__cliporaxTauriCalls);
  expect(calls.filter((call: { cmd: string }) => call.cmd === "download_app_update")).toHaveLength(1);
  expect(calls.filter((call: { cmd: string }) => call.cmd === "install_app_update")).toHaveLength(1);
});
