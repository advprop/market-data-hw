import { expect, test } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";

test("full user journey with real server and clickhouse", async ({
  page,
  request,
}) => {
  const failures: string[] = [];
  page.on("pageerror", (error) => failures.push(error.message));
  const pause = async () => {
    if (process.env.DEMO) await page.waitForTimeout(1800);
  };
  await page.goto("/");
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Server and database connected" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "import snapshots" }).click();
  await page.getByRole("button", { name: "use sample data" }).click();
  await expect(page.getByLabel("snapshot batch")).toHaveValue(/BTC-DEMO/);
  await pause();
  await page.getByLabel("write token").fill("wrong-homework-token");
  await page.getByRole("button", { name: "upload to server" }).click();
  await expect(page.getByRole("alert")).toContainText("invalid write token");
  await pause();
  await page.getByLabel("write token").fill("local-homework-token");
  await page.getByRole("button", { name: "upload to server" }).click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "snapshots saved by server: 24" }),
  ).toBeVisible();
  await pause();
  await page
    .getByRole("button", { name: "market overview", exact: true })
    .click();
  await expect(page.getByLabel("instrument")).toHaveValue("BTC-DEMO");
  await expect(
    page.getByRole("heading", { name: "order book / BTC-DEMO", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("img", { name: /mid price history/ }),
  ).toBeVisible();
  await expect(page.locator(".bid tbody tr")).toHaveCount(10);
  await mkdir("../docs/screenshots", { recursive: true });
  await pause();
  await page.screenshot({
    path: "../docs/screenshots/market.png",
    fullPage: true,
  });
  const latest = await (await request.get("/v1/book/BTC-DEMO")).json();
  expect(latest.sequence).toBe("24");
  const metrics = await (await request.get("/v1/summary/BTC-DEMO")).json();
  expect(metrics.midPrice).toBe(62019);
  await expect(page.locator(".metric").last()).toContainText("62 019");
  await page.getByLabel("depth", { exact: true }).fill("1");
  await page.getByRole("button", { name: "apply" }).click();
  await expect(page.locator(".bid tbody tr")).toHaveCount(1);
  await expect(page.locator(".ask tbody tr")).toHaveCount(1);
  await pause();
  await page.getByLabel("limit", { exact: true }).fill("3");
  await page.getByRole("button", { name: "show", exact: true }).click();
  await expect(page.getByText(/results truncated/)).toBeVisible();
  await expect(page.locator(".history-table tbody tr")).toHaveCount(3);
  await pause();
  await page.getByRole("button", { name: "full range" }).click();
  await expect(page.locator(".history-table tbody tr")).toHaveCount(24);
  await page
    .getByRole("button", { name: "open snapshot 1", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "snapshot #1", exact: true }),
  ).toBeVisible();
  await pause();
  await page.screenshot({
    path: "../docs/screenshots/history.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "close snapshot" }).click();
  const plot = page.getByRole("link", { name: /open server chart/ });
  const plotResponse = await request.get(
    (await plot.getAttribute("href")) ?? "",
  );
  expect(plotResponse.headers()["content-type"]).toContain("image/svg+xml");
  expect(await plotResponse.text()).toContain("BTC-DEMO");
  await page.getByLabel("from, inclusive").fill("2026-10-10T10:00");
  await page.getByRole("button", { name: "show", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "the start must be before the end",
  );
  await pause();
  await page.getByLabel("to, exclusive").fill("2026-10-11T10:00");
  await page.getByRole("button", { name: "show", exact: true }).click();
  await expect(
    page.getByText("no snapshots in this range.", { exact: false }),
  ).toBeVisible();
  await pause();
  await page.getByRole("button", { name: "full range" }).click();
  await expect(page.locator(".history-table tbody tr")).toHaveCount(24);
  await page.getByRole("button", { name: "import snapshots" }).click();
  await page.getByLabel("snapshot batch").fill("invalid json");
  await page.getByRole("button", { name: "upload to server" }).click();
  await expect(page.getByRole("alert")).toContainText("invalid json");
  await page.getByRole("button", { name: "use sample data" }).click();
  await page.getByLabel("write token").fill("local-homework-token");
  await page.getByRole("button", { name: "upload to server" }).click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "snapshots saved by server: 24" }),
  ).toBeVisible();
  await page.getByLabel("write token").fill("");
  await pause();
  await page.screenshot({
    path: "../docs/screenshots/import.png",
    fullPage: true,
  });
  const coverage = await (await request.get("/v1/range/BTC-DEMO")).json();
  expect(coverage.count).toBe("24");
  await page
    .getByRole("button", { name: "market overview", exact: true })
    .click();
  await expect(
    page.getByRole("img", { name: /mid price history/ }),
  ).toBeVisible();
  await page.getByLabel("depth", { exact: true }).fill("10");
  await page.getByRole("button", { name: "apply" }).click();
  await page.getByRole("button", { name: "refresh data" }).click();
  await expect(page.locator(".bid tbody tr")).toHaveCount(10);
  await pause();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    page.getByRole("heading", { name: "market overview" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "../docs/screenshots/mobile.png",
    fullPage: true,
  });
  expect(failures).toEqual([]);
});

test("empty state loading connection failure and retry", async ({ page }) => {
  await page.route("**/v1/symbols", (route) =>
    route.fulfill({ json: { symbols: [] } }),
  );
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "no snapshots yet" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "import your first snapshots" })
    .click();
  const sample = await readFile("public/snapshots.json", "utf8");
  await page.getByLabel("snapshot batch").fill(sample);
  await expect(
    page.getByRole("heading", { name: "import snapshots" }),
  ).toBeVisible();
  await page.unroute("**/v1/symbols");
  await page.route("**/health", (route) => route.abort());
  await page.getByRole("button", { name: "refresh data" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Cannot reach the server",
  );
  await page.unroute("**/health");
  await page.route("**/health", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.continue();
  });
  await page.getByRole("button", { name: "refresh data" }).click();
  await expect(page.getByRole("status")).toContainText("Checking connection");
  await expect(page.getByRole("status")).toContainText(
    "Server and database connected",
  );
});
