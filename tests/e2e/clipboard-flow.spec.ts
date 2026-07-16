import { expect, makeClipboardItems, test } from "./fixtures/tauriMock";

test("renders clipboard items and searches them", async ({ page, mockTauri }) => {
  const items = makeClipboardItems(20);
  items[6] = { ...items[6], content: "Quarterly API token note" };
  await mockTauri({ items });
  await page.goto("/");

  await expect(page.getByTestId("clipboard-card-1")).toBeVisible();
  await expect(page.getByTestId("clipboard-card-1")).toContainText(
    "Mock clipboard item 1",
  );

  await page.keyboard.press(process.platform === "darwin" ? "Meta+F" : "Control+F");
  await page.getByTestId("search-input").fill("Quarterly");

  await expect(page.getByText("Quarterly API token note")).toBeVisible();
  await expect(page.getByTestId("clipboard-card-1")).toBeHidden();
});

test("opens the editor for text items", async ({ page, mockTauri }) => {
  await mockTauri({ items: makeClipboardItems(20) });
  await page.goto("/");

  await expect(page.getByTestId("clipboard-card-1")).toBeVisible();
  await page.getByTestId("clipboard-card-1").hover();
  await page.getByTestId("clipboard-card-1-edit").click();
  await expect(page.getByTestId("content-editor")).toBeVisible();
  await expect(page.getByTestId("content-editor-textarea")).toHaveValue(
    "Mock clipboard item 1",
  );
});

test("renders and opens a 600 KB clipboard item without truncating its value", async ({
  page,
  mockTauri,
}) => {
  const startMarker = "CLIPORAX_600K_START_";
  const endMarker = "_CLIPORAX_600K_END";
  const content =
    startMarker +
    "x".repeat(600_000 - startMarker.length - endMarker.length) +
    endMarker;
  const items = makeClipboardItems(20);
  items[0] = { ...items[0], content };

  await mockTauri({ items });
  await page.goto("/");

  const card = page.getByTestId("clipboard-card-1");
  await expect(card).toBeVisible();
  await expect(card).toContainText(startMarker);
  await card.hover();
  await page.getByTestId("clipboard-card-1-edit").click();

  const textarea = page.getByTestId("content-editor-textarea");
  await expect(textarea).toBeVisible();
  await expect
    .poll(() =>
      textarea.evaluate((element: HTMLTextAreaElement) => ({
        length: element.value.length,
        startsWithMarker: element.value.startsWith("CLIPORAX_600K_START_"),
        endsWithMarker: element.value.endsWith("_CLIPORAX_600K_END"),
      })),
    )
    .toEqual({
      length: 600_000,
      startsWithMarker: true,
      endsWithMarker: true,
    });
});

test("shows the exact line count for a long clipboard item", async ({
  page,
  mockTauri,
}) => {
  const items = makeClipboardItems(20);
  items[0] = {
    ...items[0],
    content: Array.from(
      { length: 1_773 },
      (_, index) => `Long clipboard line ${index + 1}`,
    ).join("\r\n"),
  };

  await mockTauri({ items });
  await page.goto("/");

  await expect(page.getByTestId("clipboard-card-1")).toBeVisible();
  await expect(page.getByTestId("clipboard-card-1")).toContainText(
    /(?:1,773|1773) (?:lines|行)/,
  );
});

test("double-clicks an item to copy and paste it into the previous app", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({ items: makeClipboardItems(20) });
  await page.goto("/");

  const card = page.getByTestId("clipboard-card-1");
  await expect(card).toBeVisible();
  await card.dblclick();

  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__cliporaxTauriCalls
          .map((call: { cmd: string }) => call.cmd)
          .filter((command: string) =>
            [
              "clipboard_copy",
              "clipboard_move_to_top",
              "window_hide_and_paste",
            ].includes(command),
          ),
      ),
    )
    .toEqual([
      "clipboard_copy",
      "clipboard_move_to_top",
      "window_hide_and_paste",
    ]);
});

test("deletes a selected clipboard item", async ({ page, mockTauri }) => {
  await mockTauri({ items: makeClipboardItems(20) });
  await page.goto("/");

  await expect(page.getByTestId("clipboard-card-1")).toBeVisible();
  await page.getByTestId("clipboard-card-1").click();
  await page.keyboard.press("Delete");

  await expect(page.getByTestId("clipboard-card-1")).toBeHidden();
  await expect(page.getByTestId("clipboard-card-2")).toBeVisible();
});

test("surfaces IPC failures without leaving the startup screen", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({ failCommands: { tabs_get_all: "tabs failed" } });
  await page.goto("/");

  await expect(page.getByTestId("app-shell")).toBeVisible();
  await expect(page.getByTestId("clipboard-empty-state")).toBeVisible();
});
