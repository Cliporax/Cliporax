import { expect, test } from "./fixtures/tauriMock";

test("renders the main window after backend ready", async ({ page, mockTauri }) => {
  await mockTauri();
  await page.goto("/");

  await expect(page.getByTestId("app-shell")).toBeVisible();
  await expect(page.getByText("Cliporax")).toBeVisible();
  await expect(page.getByTestId("clipboard-empty-state")).toBeVisible();
});

test("shows backend readiness errors", async ({ page, mockTauri }) => {
  await mockTauri({ failCommands: { app_ready: "backend unavailable" } });
  await page.goto("/");

  await expect(page.getByText(/Backend initialization failed/)).toContainText(
    "backend unavailable",
  );
});
