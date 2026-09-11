const assert = require("node:assert/strict");
const test = require("node:test");

const { installerPlan } = require("../dist/test/install.js");

test("creates a PowerShell installer plan and an exact Windows executable path", () => {
  const plan = installerPlan("win32", { LOCALAPPDATA: "C:\\Users\\test\\AppData\\Local" });
  assert.equal(plan.shellPath, "powershell.exe");
  assert.equal(plan.executablePath, "C:\\Users\\test\\AppData\\Local\\Pinset\\bin\\pinset.exe");
  assert.match(plan.command, /install\.ps1/);
});

test("creates a POSIX installer plan for the extension host", () => {
  const plan = installerPlan("linux", { HOME: "/home/test" });
  assert.equal(plan.shellPath, undefined);
  assert.equal(plan.executablePath, "/home/test/.local/bin/pinset");
  assert.match(plan.command, /install\.sh \| sh$/);
});

test("falls back when the host has no default install directory", () => {
  assert.equal(installerPlan("win32", {}), undefined);
  assert.equal(installerPlan("linux", {}), undefined);
});
