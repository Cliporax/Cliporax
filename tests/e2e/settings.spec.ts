import { expect, test } from "./fixtures/tauriMock";

test("opens settings route and changes general settings", async ({
  page,
  mockTauri,
}) => {
  await mockTauri();
  await page.goto("/settings");

  await expect(page.getByTestId("settings-panel")).toBeVisible();
  await page.getByTestId("settings-theme-light").click();
  await page.getByTestId("settings-line-height-large").click();

  const calls = await page.evaluate(() => (window as any).__cliporaxTauriCalls);
  expect(calls.some((call: { cmd: string }) => call.cmd === "settings_update")).toBeTruthy();
});

test("opens settings from the main window control", async ({ page, mockTauri }) => {
  await mockTauri();
  await page.goto("/");

  await page.getByTestId("settings-button").click();

  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByTestId("settings-panel")).toBeVisible();
});

test("keeps settings usable when save IPC fails", async ({ page, mockTauri }) => {
  await mockTauri({ failCommands: { settings_update: "save failed" } });
  await page.goto("/settings");

  await expect(page.getByTestId("settings-panel")).toBeVisible();
  await page.getByTestId("settings-theme-light").click();
  await expect(page.getByTestId("settings-panel")).toBeVisible();
});
