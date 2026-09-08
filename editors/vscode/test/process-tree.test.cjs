const assert = require("node:assert/strict");
const test = require("node:test");

const { spawn } = require("node:child_process");

const { terminateProcessTree, terminationPlan } = require("../dist/test/process-tree.js");

test("uses a Windows tree kill and a Unix process group", () => {
  assert.deepEqual(terminationPlan(42, "win32"), {
    command: "taskkill.exe",
    args: ["/pid", "42", "/T", "/F"],
  });
  assert.deepEqual(terminationPlan(42, "linux"), { pid: -42, signal: "SIGTERM" });
});

test("terminates a detached process and its child", { timeout: 10_000 }, async () => {
  const parent = spawn(
    process.execPath,
    [
      "-e",
      "const{spawn}=require('node:child_process');const c=spawn(process.execPath,['-e','setInterval(()=>{},1000)']);console.log(c.pid);setInterval(()=>{},1000)",
    ],
    { detached: process.platform !== "win32", stdio: ["ignore", "pipe", "ignore"] },
  );
  const childPid = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("child PID was not reported")), 3_000);
    parent.stdout.once("data", (chunk) => {
      clearTimeout(timer);
      resolve(Number.parseInt(chunk.toString("utf8").trim(), 10));
    });
  });

  await terminateProcessTree(parent.pid);
  await new Promise((resolve, reject) => {
    if (parent.exitCode !== null || parent.signalCode !== null) return resolve();
    const timer = setTimeout(() => reject(new Error("parent process survived cancellation")), 3_000);
    parent.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });

  await new Promise((resolve) => setTimeout(resolve, 200));
  assert.equal(processExists(childPid), false, `child process ${childPid} survived cancellation`);
});

function processExists(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}
