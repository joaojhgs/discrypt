import { expect, test } from "playwright/test";

test("first-run recovery restores account continuity without content-key claims", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: /set up your local discrypt profile/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /recover existing user/i }).click();

  await expect(
    page.getByRole("heading", { name: /finish the local trust setup/i }),
  ).toBeVisible();
  await expect(page.getByText(/Recovered Private Lab/i).first()).toBeVisible();
  await expect(
    page.getByText(/2 authorized local devices available/i),
  ).toBeVisible();
  await expect(page.getByText(/content-key recovery/i)).toHaveCount(0);
  expect(errors).toEqual([]);
});
