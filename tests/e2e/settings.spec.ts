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

test("shows registered card and context-menu plugin actions", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({
    plugins: [
      {
        id: "com.example.translate",
        name: "Translate",
        iconDataUrl: "data:image/svg+xml;base64,PHN2Zy8+",
        extensions: [{ point: "card", component: "TranslateButton" }],
        script: "window.CliporaxPlugins = window.CliporaxPlugins || {}; window.CliporaxPlugins['com.example.translate'] = { extensions: {} };",
      },
      {
        id: "com.example.todo",
        name: "TODO",
        permissions: ["ui:context-menu"],
        extensions: [
          {
            point: "context-menu",
            component: "MoveToTodoAction",
            icon: "list-todo",
          },
        ],
        script: "window.CliporaxPlugins = window.CliporaxPlugins || {}; window.CliporaxPlugins['com.example.todo'] = { extensions: {} };",
      },
    ],
  });
  await page.goto("/settings");

  await expect(page.getByRole("button", { name: "Translate" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Translate" }).locator("img")).toBeVisible();
  await expect(page.getByRole("button", { name: "TODO" })).toBeVisible();
});

test("keeps settings usable when save IPC fails", async ({ page, mockTauri }) => {
  await mockTauri({ failCommands: { settings_update: "save failed" } });
  await page.goto("/settings");

  await expect(page.getByTestId("settings-panel")).toBeVisible();
  await page.getByTestId("settings-theme-light").click();
  await expect(page.getByTestId("settings-panel")).toBeVisible();
});
