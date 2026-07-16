import { expect, test } from "./fixtures/tauriMock";

const createdAt = "2026-01-01T00:00:00.000Z";

test("shows an insertion cursor and persists dragged tab order", async ({
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
      {
        id: 2,
        name: "Work",
        is_default: false,
        auto_capture: false,
        created_at: createdAt,
      },
      {
        id: 3,
        name: "Trash",
        is_default: false,
        auto_capture: false,
        is_trash: true,
        created_at: createdAt,
      },
    ],
  });
  await page.goto("/");

  const workTab = page.getByRole("tab", { name: /Work/ });
  const clipboardTab = page.getByRole("tab", { name: "Default" });
  await expect(page.getByRole("tab")).toHaveText(["Default", "Work", "Trash"]);

  const source = await workTab.boundingBox();
  const target = await clipboardTab.boundingBox();
  if (!source || !target) throw new Error("Tab bounds are unavailable");

  await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
  await page.mouse.down();
  await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2 + 8, {
    steps: 3,
  });
  await page.mouse.move(target.x + target.width / 2, target.y + 2, { steps: 8 });

  const indicator = page.getByTestId("tab-drop-indicator");
  await expect(indicator).toBeVisible();
  await expect(indicator).toHaveAttribute("data-position", "before");

  await page.mouse.up();

  await expect(page.getByRole("tab")).toHaveText(["Work", "Default", "Trash"]);
  await expect(indicator).toBeHidden();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__cliporaxTauriCalls.find(
          (call: { cmd: string }) => call.cmd === "tabs_reorder",
        )?.args.orderedIds,
      ),
    )
    .toEqual([2, 1, 3]);
});
