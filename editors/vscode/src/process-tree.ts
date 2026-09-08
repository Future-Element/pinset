import { spawn } from "node:child_process";

export interface TerminationPlan {
  command?: string;
  args?: string[];
  signal?: NodeJS.Signals;
  pid?: number;
}

export function terminationPlan(pid: number, platform = process.platform): TerminationPlan {
  if (platform === "win32") {
    return { command: "taskkill.exe", args: ["/pid", String(pid), "/T", "/F"] };
  }
  return { pid: -pid, signal: "SIGTERM" };
}

export async function terminateProcessTree(pid: number): Promise<void> {
  const plan = terminationPlan(pid);
  if (plan.command) {
    await new Promise<void>((resolve) => {
      const killer = spawn(plan.command!, plan.args!, { windowsHide: true, stdio: "ignore" });
      killer.once("error", () => resolve());
      killer.once("exit", () => resolve());
    });
    return;
  }
  try {
    process.kill(plan.pid!, plan.signal);
  } catch {
    return;
  }
  await new Promise((resolve) => setTimeout(resolve, 500));
  try {
    process.kill(plan.pid!, "SIGKILL");
  } catch {
    // The process group already exited.
  }
}
