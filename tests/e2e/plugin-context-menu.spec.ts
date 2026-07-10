import { expect, makeClipboardItems, test } from "./fixtures/tauriMock";

const todoContextMenuPlugin = {
  id: "com.cliporax.todo",
  name: "TODO",
  permissions: ["ui:context-menu", "data:read", "data:delete"],
  extensions: [
    {
      point: "context-menu",
      component: "MoveToTodoAction",
      icon: "list-todo",
      condition: 'item.type === "text"',
      priority: 80,
    },
  ],
  script: `
    window.__todoMovedItems = [];
    window.CliporaxPlugins = window.CliporaxPlugins || {};
    window.CliporaxPlugins["com.cliporax.todo"] = {
      onActivate() {},
      onDeactivate() {},
      extensions: {
        MoveToTodoAction: {
          getMenuItems() {
            return [{
              id: "move-to-todo",
              label: "Move to TODO",
              icon: "list-todo",
              action: async (api) => {
                const items = api.getItems();
                window.__todoMovedItems.push(...items.map((item) => item.content));
                await api.deleteItems(items.map((item) => item.id).filter((id) => typeof id === "number"));
              },
            }];
          },
        },
      },
    };
  `,
};

test("shows TODO context-menu action for text items and executes it", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({
    items: makeClipboardItems(3),
    plugins: [todoContextMenuPlugin],
  });
  await page.goto("/");

  const firstCard = page.getByTestId("clipboard-card-1");
  await expect(firstCard).toBeVisible();
  await firstCard.click({ button: "right" });

  const moveToTodo = page.getByRole("button", { name: "Move to TODO" });
  await expect(moveToTodo).toBeVisible();
  await expect(moveToTodo.locator("svg")).toBeVisible();
  await moveToTodo.click();

  await expect(firstCard).toBeHidden();
  await expect(page.getByTestId("clipboard-card-2")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => (window as any).__todoMovedItems ?? []),
    )
    .toEqual(["Mock clipboard item 1"]);

  const calls = await page.evaluate(() => (window as any).__cliporaxTauriCalls);
  expect(calls).toContainEqual(
    expect.objectContaining({
      cmd: "clipboard_delete_by_ids",
      args: expect.objectContaining({ ids: [1] }),
    }),
  );
});
