import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDir, "..");
const executable = resolve(process.argv[2] ?? resolve(repositoryRoot, "helper", "target", "x86_64-pc-windows-gnullvm", "release", "magic-corners-helper.exe"));
const tempParent = tmpdir();
const smokeRoot = resolve(process.env.MAGIC_CORNERS_SMOKE_ROOT ?? tempParent, `convenient-window-instance-${process.pid}`);
const firstDataDir = resolve(smokeRoot, "utools-data");
const secondDataDir = resolve(smokeRoot, "desktop-data");
const children = new Map();

if (!isAbsolute(executable) || !existsSync(executable)) {
  throw new Error(`helper executable does not exist: ${executable}`);
}
mkdirSync(firstDataDir, { recursive: true });
mkdirSync(secondDataDir, { recursive: true });

try {
  const first = startHelper(firstDataDir);
  const firstToken = await waitForToken(firstDataDir);
  await waitForReady(firstToken);

  const conflicting = startHelper(secondDataDir);
  const conflictExit = await waitForExit(conflicting, 5000);
  if (conflictExit === 0) throw new Error("second helper unexpectedly exited successfully");
  const conflictLog = await waitForLog(secondDataDir);
  if (!conflictLog.includes("HELPER_INSTANCE_CONFLICT")) {
    throw new Error(`second helper did not report the instance conflict (exit ${conflictExit})`);
  }
  console.log(`intentional conflict: exit=${conflictExit}, marker=HELPER_INSTANCE_CONFLICT`);

  await stopHelper(firstToken);
  const firstExit = await waitForExit(first, 5000);
  if (firstExit !== 0) throw new Error(`first helper did not stop cleanly: exit ${firstExit}`);

  const recovered = startHelper(secondDataDir);
  const recoveredToken = await waitForToken(secondDataDir);
  await waitForReady(recoveredToken);
  await stopHelper(recoveredToken);
  const recoveredExit = await waitForExit(recovered, 5000);
  if (recoveredExit !== 0) throw new Error(`recovered helper did not stop cleanly: exit ${recoveredExit}`);
  console.log(`recovery start: exit=${recoveredExit}, dataDir=${secondDataDir}`);
  console.log("helper instance smoke: passed");
} finally {
  for (const [child, dataDir] of children) {
    if (child.exitCode !== null) continue;
    const tokenPath = resolve(dataDir, "auth-token");
    try {
      if (existsSync(tokenPath)) await stopHelper(readFileSync(tokenPath, "utf8").trim());
      await waitForExit(child, 5000);
    } catch (error) {
      console.error(`helper process ${child.pid} is still running: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  const allExited = [...children.keys()].every((child) => child.exitCode !== null);
  if (allExited) rmSync(smokeRoot, { recursive: true, force: true });
  else console.error(`temporary data kept at ${smokeRoot}`);
}

function startHelper(dataDir) {
  const child = spawn(executable, ["--data-dir", dataDir], {
    cwd: dirname(executable),
    windowsHide: true,
    stdio: "ignore"
  });
  children.set(child, dataDir);
  child.once("exit", () => children.delete(child));
  return child;
}

async function waitForToken(dataDir) {
  const tokenPath = resolve(dataDir, "auth-token");
  await waitUntil(() => existsSync(tokenPath) && readFileSync(tokenPath, "utf8").trim().length === 64, 5000, `token at ${tokenPath}`);
  return readFileSync(tokenPath, "utf8").trim();
}

async function waitForLog(dataDir) {
  const logPath = resolve(dataDir, "magic-corners-helper.log");
  await waitUntil(() => existsSync(logPath), 3000, `log at ${logPath}`);
  return readFileSync(logPath, "utf8");
}

function waitForReady(token) {
  return new Promise((resolveReady, reject) => {
    const socket = new WebSocket("ws://127.0.0.1:56873", token);
    const timeout = setTimeout(() => {
      socket.close();
      reject(new Error("helper.ready timed out"));
    }, 5000);
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (message.type !== "helper.ready") return;
      clearTimeout(timeout);
      socket.close();
      resolveReady();
    });
    socket.addEventListener("error", () => {
      clearTimeout(timeout);
      reject(new Error("helper readiness connection failed"));
    }, { once: true });
  });
}

function stopHelper(token) {
  return new Promise((resolveStop, reject) => {
    const socket = new WebSocket("ws://127.0.0.1:56873", token);
    const timeout = setTimeout(() => {
      socket.close();
      reject(new Error("helper stop timed out"));
    }, 5000);
    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({
        id: crypto.randomUUID(),
        type: "helper.stop",
        time: Date.now(),
        data: {}
      }));
    });
    socket.addEventListener("close", () => {
      clearTimeout(timeout);
      resolveStop();
    }, { once: true });
    socket.addEventListener("error", () => {
      clearTimeout(timeout);
      reject(new Error("helper stop connection failed"));
    }, { once: true });
  });
}

function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null) return Promise.resolve(child.exitCode);
  return new Promise((resolveExit, reject) => {
    const timeout = setTimeout(() => reject(new Error(`helper process ${child.pid} did not exit`)), timeoutMs);
    child.once("exit", (code) => {
      clearTimeout(timeout);
      resolveExit(code ?? -1);
    });
    child.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
  });
}

async function waitUntil(predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 50));
  }
  throw new Error(`timed out waiting for ${label}`);
}
