import { expect, makeClipboardItems, test } from "./fixtures/tauriMock";

const createdAt = "2026-01-01T00:00:00.000Z";

test("can resize and restore the collapsed clipboard collections sidebar", async ({
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

  const expandedHandleBounds = await handle.boundingBox();
  if (!expandedHandleBounds) throw new Error("Expanded sidebar handle is unavailable");
  await page.mouse.move(expandedHandleBounds.x + 1, expandedHandleBounds.y + 40);
  await page.mouse.down();
  await page.mouse.move(20, expandedHandleBounds.y + 40);
  await page.mouse.up();
  await expect(sidebar).toBeHidden();

  const collapsedHandle = page.getByTestId("collapsed-clipboard-sidebar-resize-handle");
  const collapsedHandleBounds = await collapsedHandle.boundingBox();
  if (!collapsedHandleBounds) throw new Error("Collapsed sidebar handle is unavailable");
  await page.mouse.move(collapsedHandleBounds.x + 1, collapsedHandleBounds.y + 40);
  await page.mouse.down();
  await page.mouse.move(collapsedHandleBounds.x + 100, collapsedHandleBounds.y + 40);
  await page.mouse.up();
  await expect(sidebar).toBeVisible();
});

test("clears multi-selection when switching clipboard collections", async ({
  page,
  mockTauri,
}) => {
  const items = makeClipboardItems(2);
  items.push({
    ...items[0],
    id: 3,
    content: "Work clipboard item",
    content_hash: "hash-3",
    tab_id: 2,
  });

  await mockTauri({
    items,
    tabs: [
      {
        id: 1,
        name: "Clipboard",
        is_default: true,
        auto_capture: true,
        created_at: createdAt,
      },
      {
        id: 2,
        name: "Work",
        is_default: false,
        auto_capture: false,
        created_at: createdAt,
      },
    ],
  });
  await page.goto("/");

  await page.getByTestId("clipboard-card-1").click({ modifiers: ["Control"] });
  await page.getByTestId("clipboard-card-2").click({ modifiers: ["Control"] });
  await expect(page.getByTestId("clipboard-card-1")).toHaveAttribute(
    "data-multi-selected",
    "true",
  );

  await page.getByRole("tab", { name: "Work" }).click();

  await expect(page.getByTestId("clipboard-card-3")).toBeVisible();
  await expect(page.getByTestId("clipboard-card-3")).toHaveAttribute(
    "data-multi-selected",
    "false",
  );
});
