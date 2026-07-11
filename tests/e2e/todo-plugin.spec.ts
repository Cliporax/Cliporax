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
  await page.getByLabel("New TODO group").fill("Work");
  await page.getByRole("button", { name: "Add group" }).click();
  await page.getByRole("button", { name: "Create TODO item" }).click();
  await page.getByRole("textbox", { name: "Add TODO item" }).fill("Prepare release notes");
  await page.getByRole("button", { name: "Save new TODO item" }).click();

  await expect(page.getByText("Prepare release notes")).toBeVisible();

  await page
    .getByLabel("TODO item: Prepare release notes")
    .dragTo(page.getByRole("button", { name: "Show TODO group Inbox" }));
  await expect(page.getByText("Prepare release notes")).toHaveCount(0);

  await page.getByRole("button", { name: "Show TODO group Inbox" }).click();
  await expect(page.getByText("Prepare release notes")).toBeVisible();

  await page.getByRole("button", { name: "Edit TODO: Prepare release notes" }).click();
  await page
    .getByRole("textbox", { name: "Edit TODO: Prepare release notes" })
    .fill("Prepare release notes v2");
  await page.getByRole("button", { name: "Save TODO: Prepare release notes" }).click();

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
  await page.getByRole("button", { name: "Save new TODO item" }).click();

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
