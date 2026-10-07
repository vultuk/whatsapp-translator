import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import path from "node:path";
async function open(page: Page, query = "") {
  await page.route("http://127.0.0.1:4179/**", async (route) => {
    const file =
      new URL(route.request().url()).pathname === "/app.html"
        ? "dist/chats.html"
        : "tests/host.html";
    await route.fulfill({
      contentType: "text/html",
      headers:
        file === "dist/chats.html"
          ? {
              "Content-Security-Policy":
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'",
            }
          : {},
      body: await readFile(path.join(import.meta.dirname, "..", file), "utf8"),
    });
  });
  await page.goto(`http://127.0.0.1:4179/${query}`);
  return page.frameLocator("#app");
}
async function calls(page: Page, name: string) {
  return page.evaluate(
    (name) =>
      (
        window as unknown as {
          calls: { name: string; args: Record<string, unknown> }[];
        }
      ).calls.filter((call) => call.name === name),
    name,
  );
}

test("registered bundle renders initial results once, pagination, originals and drafts", async ({
  page,
}) => {
  const ui = await open(page);
  await expect(
    ui.getByRole("button", { name: "Open chat with Alice Laurent" }),
  ).toBeVisible();
  expect(await calls(page, "list_contacts")).toHaveLength(0);
  await ui.getByRole("button", { name: "Load more conversations" }).click();
  await expect(
    ui.getByRole("button", { name: "Open chat with Daniel Chen" }),
  ).toBeVisible();
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(ui.getByText("Good morning!", { exact: false })).toBeVisible();
  await ui.getByText("Show original", { exact: true }).first().click();
  await expect(
    ui.getByText(
      "Bonjour ! Tu as pu regarder le petit café que je t’ai envoyé ?",
      { exact: true },
    ),
  ).toBeVisible();
  await ui
    .getByRole("textbox", { name: "Write a message" })
    .fill("Draft for Alice");
  await ui.getByRole("button", { name: "Open chat with Daniel Chen" }).click();
  await expect(
    ui.getByText("No messages in this conversation yet."),
  ).toBeVisible();
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(
    ui.getByRole("textbox", { name: "Write a message" }),
  ).toHaveValue("Draft for Alice");
  await ui.getByRole("button", { name: "Load older messages" }).click();
  await expect(
    ui.getByText("Earlier conversation", { exact: true }),
  ).toBeVisible();
  expect(await calls(page, "mark_conversation_read")).toHaveLength(0);
  expect(await calls(page, "send_message")).toHaveLength(0);
});
test("slow chat responses cannot replace a newer selected chat", async ({
  page,
}) => {
  const ui = await open(page, "?slowChat=1");
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(ui.getByText("Loading conversation…")).toBeVisible();
  await ui
    .getByRole("button", { name: "Open chat with Weekend in Lisbon" })
    .click();
  await expect(
    ui.getByText("No messages in this conversation yet."),
  ).toBeVisible();
  await page.waitForTimeout(700);
  await expect(
    ui.getByRole("heading", { name: "Weekend in Lisbon" }),
  ).toBeVisible();
  await expect(ui.getByText("Good morning!", { exact: false })).toHaveCount(0);
});
test("prepare, explicit confirmation, retry same send key and reply tools", async ({
  page,
}) => {
  const ui = await open(page, "?sendError=1");
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await ui.getByRole("button", { name: "Reply", exact: true }).first().click();
  await ui
    .getByRole("textbox", { name: "Write a message" })
    .fill("See you soon!");
  await ui.getByRole("button", { name: "Prepare message for review" }).click();
  await expect(ui.getByRole("dialog")).toBeVisible();
  await expect(ui.locator("#confirm-text")).toHaveText("À bientôt !");
  expect(await calls(page, "reply_to_message")).toHaveLength(0);
  await ui.getByRole("button", { name: "Send message", exact: true }).click();
  await expect(ui.locator("#send-error")).toContainText("Synthetic timeout");
  await ui.getByRole("button", { name: "Send message", exact: true }).click();
  await expect(ui.getByRole("dialog")).not.toBeVisible();
  const sends = await calls(page, "reply_to_message");
  expect(sends).toHaveLength(2);
  expect(sends[0].args.idempotency_key).toEqual(sends[1].args.idempotency_key);
  expect(
    (await calls(page, "prepare_message"))[0].args.reply_to_message_id,
  ).toBe("a1");
  await expect(
    ui.getByRole("textbox", { name: "Write a message" }),
  ).toHaveValue("");
});
test("edited draft invalidates slow preparation and host interruption recovers", async ({
  page,
}) => {
  const ui = await open(page, "?slowPrepare=1");
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await ui.getByRole("textbox", { name: "Write a message" }).fill("Before");
  await ui.getByRole("button", { name: "Prepare message for review" }).click();
  await ui.getByRole("textbox", { name: "Write a message" }).fill("After");
  await page.waitForTimeout(600);
  await expect(ui.getByRole("dialog")).not.toBeVisible();
  await page.evaluate(() =>
    (
      window as unknown as { notify: (method: string, params: object) => void }
    ).notify("ui/notifications/tool-cancelled", { reason: "interrupted" }),
  );
  await expect(ui.getByRole("alert")).toContainText("interrupted");
  await ui.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(
    ui.getByRole("button", { name: "Open chat with Alice Laurent" }),
  ).toBeVisible();
  expect(await calls(page, "send_message")).toHaveLength(0);
});
test("read-only scope, empty search and read/translate/reaction actions", async ({
  page,
}) => {
  const ui = await open(page, "?readonly=1");
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(
    ui.getByText("Read-only connection", { exact: false }),
  ).toBeVisible();
  await expect(
    ui.getByRole("textbox", { name: "Write a message" }),
  ).not.toBeVisible();
  await expect(
    ui.getByRole("button", { name: "React with heart" }),
  ).toHaveCount(0);
  await ui
    .getByRole("searchbox", { name: "Search conversations" })
    .fill("Nobody");
  await expect(
    ui.getByText("No conversations match your search."),
  ).toBeVisible();
});
test("actions use independent tools and errors recover", async ({ page }) => {
  const ui = await open(page, "?error=1");
  await ui.getByRole("button", { name: "Refresh conversations" }).click();
  await expect(ui.getByRole("alert")).toContainText(
    "Synthetic connection failure",
  );
  await ui.getByRole("button", { name: "Retry", exact: true }).click();
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await ui.getByRole("button", { name: "Translate", exact: true }).click();
  await expect(ui.getByText("See you soon!", { exact: true })).toBeVisible();
  await ui.getByRole("button", { name: "React with heart" }).first().click();
  await ui.getByRole("button", { name: "Mark conversation read" }).click();
  expect(await calls(page, "translate_message")).toHaveLength(1);
  expect(await calls(page, "react_to_message")).toHaveLength(1);
  expect(await calls(page, "mark_conversation_read")).toHaveLength(1);
});
test("deep link opens authorised target and narrow layout navigates back", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const ui = await open(page, "?deep=1");
  await expect(ui.getByRole("heading", { name: "Daniel Chen" })).toBeVisible();
  await ui.getByRole("button", { name: "Back to chats" }).click();
  await expect(
    ui.getByRole("heading", { name: "Chats", exact: true }),
  ).toBeVisible();
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(
    ui.getByRole("textbox", { name: "Write a message" }),
  ).toBeVisible();
  const overflow = await ui
    .locator("body")
    .evaluate((el) => el.scrollWidth > window.innerWidth);
  expect(overflow).toBe(false);
  await page.screenshot({
    path: path.join(
      import.meta.dirname,
      "../../../output/playwright/extension-mobile.png",
    ),
  });
});
test("visual desktop, dark and empty states", async ({ page }) => {
  let ui = await open(page);
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(ui.getByText("Perfect!", { exact: false })).toBeVisible();
  await page.screenshot({
    path: path.join(
      import.meta.dirname,
      "../../../output/playwright/extension-desktop.png",
    ),
  });
  ui = await open(page, "?dark=1");
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await expect(ui.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(ui.locator("body")).toHaveCSS("color", "rgb(233, 239, 236)");
  await page.screenshot({
    path: path.join(
      import.meta.dirname,
      "../../../output/playwright/extension-dark.png",
    ),
  });
  ui = await open(page, "?empty=1");
  await expect(ui.getByText("No conversations here yet.")).toBeVisible();
  await page.screenshot({
    path: path.join(
      import.meta.dirname,
      "../../../output/playwright/extension-empty.png",
    ),
  });
});

test("latest search wins and sparse filters remain navigable", async ({
  page,
}) => {
  const ui = await open(page, "?slowSearch=1");
  await expect(
    ui.getByRole("heading", { name: "Chats", exact: true }),
  ).toBeVisible();
  await ui
    .getByRole("searchbox", { name: "Search conversations" })
    .fill("Alice");
  await expect
    .poll(async () =>
      (await calls(page, "list_contacts")).some(
        (call) => call.args.query === "Alice",
      ),
    )
    .toBe(true);
  await ui
    .getByRole("searchbox", { name: "Search conversations" })
    .fill("Daniel");
  await expect(
    ui.getByRole("button", { name: "Open chat with Daniel Chen" }),
  ).toBeVisible();
  await page.waitForTimeout(650);
  await expect(
    ui.getByRole("button", { name: "Open chat with Alice Laurent" }),
  ).toHaveCount(0);
  await ui.getByRole("searchbox", { name: "Search conversations" }).fill("");
  await ui.getByRole("button", { name: "Groups", exact: true }).click();
  await expect(
    ui.getByRole("button", { name: "Open chat with Weekend in Lisbon" }),
  ).toBeVisible();
  await expect(
    ui.getByRole("button", { name: "Open chat with Alice Laurent" }),
  ).toHaveCount(0);
});
test("untrusted names stay text and forged iframe notifications are rejected", async ({
  page,
}) => {
  const ui = await open(page, "?xss=1");
  await expect(
    ui.getByRole("button", {
      name: "Open chat with <img src=x onerror=alert(1)>",
    }),
  ).toBeVisible();
  await expect(ui.locator("img")).toHaveCount(0);
  await ui.locator("body").evaluate(() =>
    window.postMessage(
      {
        jsonrpc: "2.0",
        method: "ui/notifications/tool-result",
        params: {
          content: [],
          structuredContent: {
            contacts: [],
            permissions: { read: true, send: true },
          },
        },
      },
      "*",
    ),
  );
  await expect(
    ui.getByRole("button", {
      name: "Open chat with <img src=x onerror=alert(1)>",
    }),
  ).toBeVisible();
});
test("cancelled and expired previews never call a send tool", async ({
  page,
}) => {
  const ui = await open(page, "?expired=1");
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await ui.getByRole("textbox", { name: "Write a message" }).fill("Hello");
  await ui.getByRole("button", { name: "Prepare message for review" }).click();
  await expect(ui.getByRole("dialog")).toBeVisible();
  await ui.getByRole("button", { name: "Send message", exact: true }).click();
  await expect(ui.locator("#send-error")).toContainText("expired");
  await ui.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(await calls(page, "send_message")).toHaveLength(0);
  await expect(
    ui.getByRole("textbox", { name: "Write a message" }),
  ).toHaveValue("Hello");
});

test("refresh during preparation releases controls and theme updates keep chat selection", async ({
  page,
}) => {
  const ui = await open(page, "?slowPrepare=1&deep=1");
  await expect(ui.getByRole("heading", { name: "Daniel Chen" })).toBeVisible();
  await ui
    .getByRole("button", { name: "Open chat with Alice Laurent" })
    .click();
  await ui.getByRole("textbox", { name: "Write a message" }).fill("Hello");
  await ui.getByRole("button", { name: "Prepare message for review" }).click();
  await ui.getByRole("button", { name: "Refresh conversations" }).click();
  await expect(
    ui.getByRole("button", { name: "Prepare message for review" }),
  ).toBeEnabled();
  await expect(ui.getByRole("dialog")).not.toBeVisible();
  await page.evaluate(() =>
    (
      window as unknown as { notify: (method: string, params: object) => void }
    ).notify("ui/notifications/host-context-changed", { theme: "dark" }),
  );
  await expect(ui.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(
    ui.getByRole("heading", { name: "Alice Laurent" }),
  ).toBeVisible();
});
