import { expect, makeClipboardItems, test } from "./fixtures/tauriMock";

const pluginIconDataUrl =
  "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAxNiAxNiI+PHJlY3Qgd2lkdGg9IjE2IiBoZWlnaHQ9IjE2IiByeD0iNCIgZmlsbD0iI2ZmMDAwMCIvPjwvc3ZnPg==";

const todoCardActionPlugin = {
  id: "com.cliporax.todo",
  name: "TODO",
  permissions: ["ui:extension", "data:read", "data:delete"],
  extensions: [
    {
      point: "card",
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

const hiddenCardActionPlugins = [
  {
    id: "com.cliporax.qrcode",
    name: "QR Code",
    iconDataUrl: pluginIconDataUrl,
    permissions: ["ui:extension"],
    extensions: [
      {
        point: "card",
        component: "GenerateQrAction",
        icon: "qr-code",
        priority: 10,
        condition: 'position === "action"',
      },
    ],
    script:
      'window.CliporaxPlugins = window.CliporaxPlugins || {}; window.__qrcodeClicks = 0; window.CliporaxPlugins["com.cliporax.qrcode"] = { extensions: { GenerateQrAction: { shouldShow(props) { return props.data.position === "action"; }, render(props) { if (props.data.position !== "action") return null; const button = document.createElement("button"); button.addEventListener("click", () => window.__qrcodeClicks += 1); return button; } } } };',
  },
  {
    id: "com.cliporax.translate",
    name: "Translate",
    iconDataUrl: pluginIconDataUrl,
    permissions: ["ui:extension"],
    extensions: [
      {
        point: "card",
        component: "TranslateButton",
        icon: "languages",
        priority: 30,
        condition: 'item.type === "text"',
      },
    ],
    script:
      'window.CliporaxPlugins = window.CliporaxPlugins || {}; window.__translateClicks = 0; window.CliporaxPlugins["com.cliporax.translate"] = { extensions: { TranslateButton: { render() { const button = document.createElement("button"); button.addEventListener("click", () => window.__translateClicks += 1); return button; } } } };',
  },
];

test("shows a card plugin action in the context menu and executes it", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({
    items: makeClipboardItems(3),
    plugins: [todoCardActionPlugin],
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

test("keeps hidden card plugin actions available from the context menu", async ({
  page,
  mockTauri,
}) => {
  await mockTauri({
    items: makeClipboardItems(1),
    plugins: hiddenCardActionPlugins,
    settings: {
      plugin_action_visibility: {
        "com.cliporax.qrcode:card:GenerateQrAction": false,
        "com.cliporax.translate:card:TranslateButton": false,
      },
    },
  });
  await page.goto("/");

  await page.getByTestId("clipboard-card-1").click({ button: "right" });

  await expect(
    page.getByRole("button", { name: "QR Code", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Translate", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "QR Code", exact: true }).locator("img"),
  ).toHaveAttribute("src", pluginIconDataUrl);
  await expect(
    page.getByRole("button", { name: "Translate", exact: true }).locator("img"),
  ).toHaveAttribute("src", pluginIconDataUrl);
  const menuActions = await page
    .locator(".fixed > button")
    .allTextContents();
  expect(menuActions.indexOf("Translate")).toBeLessThan(
    menuActions.indexOf("QR Code"),
  );

  await page.getByRole("button", { name: "QR Code", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__qrcodeClicks))
    .toBe(1);

  await page.getByTestId("clipboard-card-1").click({ button: "right" });
  await page.getByRole("button", { name: "Translate", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__translateClicks))
    .toBe(1);
});
