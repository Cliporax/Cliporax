import { expect, test } from "./fixtures/tauriMock";

const createdAt = "2026-01-01T00:00:00.000Z";

test("can resize, hide, and restore the clipboard collections sidebar", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({
    tabs: [
      {
        id: 1,
        name: "Clipboard",
        is_default: true,
        auto_capture: true,
        created_at: createdAt,
      },
    ],
  });
  await page.goto("/");

  const sidebar = page.getByTestId("clipboard-tab-sidebar");
  const initialWidth = (await sidebar.boundingBox())?.width;
  const handle = page.getByTestId("clipboard-sidebar-resize-handle");
  const handleBounds = await handle.boundingBox();
  if (!initialWidth || !handleBounds) throw new Error("Sidebar bounds are unavailable");

  await page.mouse.move(handleBounds.x + 1, handleBounds.y + 40);
  await page.mouse.down();
  await page.mouse.move(handleBounds.x + 61, handleBounds.y + 40);
  await page.mouse.up();
  await expect.poll(async () => (await sidebar.boundingBox())?.width).toBeGreaterThan(initialWidth);

  await page.getByRole("button", { name: "Hide clipboard collections" }).click();
  await expect(sidebar).toBeHidden();

  await page.getByRole("button", { name: "Show clipboard collections" }).click();
  await expect(sidebar).toBeVisible();
});
