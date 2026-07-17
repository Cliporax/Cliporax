import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "./fixtures/tauriMock";

const todoScriptPath = resolve(
  process.cwd(),
  "../CliporaxPlugins/plugins/com.cliporax.todo/main.js",
);

const todoPlugin = () => ({
  id: "com.cliporax.todo",
  name: "TODO",
  permissions: [
    "ui:extension",
    "system:storage",
    "ui:context-menu",
    "data:read",
    "data:delete",
  ],
  extensions: [
    {
      point: "content-tab",
      component: "TodoView",
      icon: "list-todo",
      priority: 40,
    },
  ],
  script: readFileSync(todoScriptPath, "utf8"),
});

test("TODO plugin supports grouped, movable, editable items with tab icon", async ({
  page,
  mockTauri,
}) => {
  test.skip(!existsSync(todoScriptPath), "TODO plugin build output is missing");

  await mockTauri({
    items: [],
    plugins: [todoPlugin()],
  });

  await page.goto("/");

  const todoTab = page.getByRole("button", { name: "TODO" });
  await expect(todoTab).toBeVisible();
  await expect(todoTab.locator("svg")).toBeVisible();
  await todoTab.click();

  await expect(page.locator("select")).toHaveCount(0);

  await expect(page.locator(".todo-pro-input-grid")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add group" })).toHaveCount(0);

  await page.getByRole("button", { name: "Create TODO group" }).click();
  const groupInput = page.getByLabel("New TODO group");
  await expect(groupInput).toHaveCSS("font-size", "12px");
  await expect(groupInput).toHaveCSS("height", "32px");
  await groupInput.fill("Work");
  await groupInput.press("Enter");
  await page.getByRole("button", { name: "Create TODO item" }).click();
  const addItemInput = page.getByRole("textbox", { name: "Add TODO item" });
  await expect(addItemInput).toHaveCSS("font-size", "12px");
  await expect(addItemInput).toHaveCSS("height", "40px");
  await addItemInput.fill("Prepare release notes");
  await addItemInput.press("Enter");

  await expect(page.getByText("Prepare release notes")).toBeVisible();

  await page
    .getByLabel("TODO item: Prepare release notes")
    .dragTo(page.getByRole("button", { name: "Show TODO group Inbox" }));
  await expect(page.getByText("Prepare release notes")).toHaveCount(0);

  await page.getByRole("button", { name: "Show TODO group Inbox" }).click();
  await expect(page.getByText("Prepare release notes")).toBeVisible();

  await page.getByRole("button", { name: "Edit TODO: Prepare release notes" }).click();
  const editor = page.getByRole("textbox", { name: "Edit TODO: Prepare release notes" });
  await expect(editor).toBeVisible();
  const editorLayout = await editor.evaluate((element) => {
    const editorBounds = element.getBoundingClientRect();
    const rowBounds = element.closest(".todo-pro-item")?.getBoundingClientRect();
    return {
      rightInset: rowBounds ? Math.round(rowBounds.right - editorBounds.right) : Number.POSITIVE_INFINITY,
      height: Math.round(editorBounds.height),
      viewportHeight: window.innerHeight,
    };
  });
  expect(editorLayout.rightInset).toBeLessThanOrEqual(12);
  expect(editorLayout.height).toBeLessThanOrEqual(editorLayout.viewportHeight * 0.34 + 1);
  await editor.press("Escape");
  await expect(editor).toHaveCount(0);
  await expect(page.getByRole("button", { name: "TODO", exact: true })).toBeVisible();
  const escapeCalls = await page.evaluate(() =>
    (window as any).__cliporaxTauriCalls.filter(
      (call: { cmd: string }) => call.cmd === "window_hide",
    ),
  );
  expect(escapeCalls).toHaveLength(0);

  await page.getByRole("button", { name: "Edit TODO: Prepare release notes" }).click();
  const reopenedEditor = page.getByRole("textbox", { name: "Edit TODO: Prepare release notes" });
  await expect(reopenedEditor).toBeVisible();
  await reopenedEditor.fill("Prepare release notes v2");
  await reopenedEditor.press("Enter");

  await expect(page.getByText("Prepare release notes v2")).toBeVisible();
  await expect(page.getByText("Prepare release notes", { exact: true })).toHaveCount(0);

  await page.getByLabel("TODO item: Prepare release notes v2").focus();
  await page.keyboard.press("Delete");
  await expect(page.getByText("Prepare release notes v2")).toHaveCount(0);

  await page.getByRole("button", { name: "Delete TODO group Work" }).click();
  await expect(page.getByRole("button", { name: "Show TODO group Work" })).toHaveCount(0);
});

test("TODO plugin layout stays usable at compact width", async ({
  page,
  mockTauri,
}) => {
  test.skip(!existsSync(todoScriptPath), "TODO plugin build output is missing");

  await page.setViewportSize({ width: 390, height: 720 });
  await mockTauri({
    items: [],
    plugins: [todoPlugin()],
  });
  await page.goto("/");

  await page.getByRole("button", { name: "TODO" }).click();
  await page.getByRole("button", { name: "Create TODO item" }).click();
  await page.getByRole("textbox", { name: "Add TODO item" }).fill("Compact layout task");
  await page.getByRole("textbox", { name: "Add TODO item" }).press("Enter");

  await expect(page.getByText("Compact layout task")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => ({
        body: document.body.scrollWidth <= document.body.clientWidth,
        root:
          document.documentElement.scrollWidth <=
          document.documentElement.clientWidth,
      })),
    )
    .toEqual({ body: true, root: true });
});
