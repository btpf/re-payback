// Web-build smoke test: drives headless Chrome over the DevTools protocol,
// loads a URL, logs the page console and runs steps (docs/web.md "Testing").
// usage: node tools/checks/web_smoke.mjs <url> <profileDir> <steps...>
//   e.g. node tools/checks/web_smoke.mjs "http://localhost:8080/?map=0" scratch/chrome-profile wait:20000 shot:out/web.png
//   steps: wait:<ms>  shot:<png>  key:<Key>[x<n>]  eval:<js>  click:<x>,<y>
// <profileDir> is Chrome's --user-data-dir: reuse it to check that saves and
// settings (localStorage) survive a reload; keep it under scratch/ (it holds
// browser state, never commit it). CHROME overrides the browser path.
// Needs Node 22+ (global WebSocket).
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const [url, profile, ...steps] = process.argv.slice(2);
if (!url || !profile) {
  console.error("usage: node tools/checks/web_smoke.mjs <url> <profileDir> <steps...>");
  process.exit(2);
}
const chrome = process.env.CHROME || "C:/Program Files/Google/Chrome/Application/chrome.exe";
const port = 9300 + Math.floor(Math.random() * 500);
const proc = spawn(chrome, [
  "--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
  "--window-size=1280,800", "--enable-unsafe-swiftshader", "--use-angle=swiftshader",
  "--autoplay-policy=no-user-gesture-required", "--no-first-run", "--no-default-browser-check",
  "about:blank",
], { stdio: "ignore" });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let ws;
for (let i = 0; i < 50; i++) {
  try {
    const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    const page = list.find((t) => t.type === "page");
    if (page) { ws = new WebSocket(page.webSocketDebuggerUrl); break; }
  } catch {}
  await sleep(200);
}
await new Promise((r) => ws.addEventListener("open", r));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
  if (m.method === "Runtime.consoleAPICalled") {
    const text = m.params.args.map((a) => a.value ?? a.description ?? "").join(" ");
    console.log(`[console.${m.params.type}] ${text}`.slice(0, 400));
  } else if (m.method === "Runtime.exceptionThrown") {
    console.log(`[exception] ${JSON.stringify(m.params.exceptionDetails).slice(0, 600)}`);
  } else if (m.method === "Log.entryAdded") {
    console.log(`[log.${m.params.entry.level}] ${m.params.entry.text}`.slice(0, 400));
  }
});
const send = (method, params = {}) => new Promise((r) => {
  const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params }));
});
await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");
await send("Page.navigate", { url });
for (const s of steps) {
  const [cmd, ...rest] = s.split(":");
  const arg = rest.join(":");
  if (cmd === "wait") await sleep(Number(arg));
  else if (cmd === "shot") {
    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(arg, Buffer.from(r.result.data, "base64"));
    console.log(`[shot] ${arg}`);
  } else if (cmd === "key") {
    const [key, n] = arg.split("x");
    const codes = { Enter: 13, ArrowDown: 40, ArrowUp: 38, Escape: 27, ArrowLeft: 37, ArrowRight: 39, " ": 32 };
    for (let i = 0; i < Number(n || 1); i++) {
      const code = key.length === 1 ? `Key${key.toUpperCase()}` : key;
      const p = { key, code, windowsVirtualKeyCode: codes[key] ?? key.toUpperCase().charCodeAt(0) };
      await send("Input.dispatchKeyEvent", { type: "keyDown", ...p, text: key === "Enter" ? "\r" : key.length === 1 ? key : undefined });
      await sleep(120);
      await send("Input.dispatchKeyEvent", { type: "keyUp", ...p });
      await sleep(400);
    }
  } else if (cmd === "click") {
    const [x, y] = arg.split(",").map(Number);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
    }
  } else if (cmd === "eval") {
    const r = await send("Runtime.evaluate", { expression: arg, returnByValue: true, awaitPromise: true });
    console.log(`[eval] ${JSON.stringify(r.result.result.value ?? r.result)}`.slice(0, 2000));
  }
}
// Leave the page first so localStorage is flushed, then close Chrome
// cleanly (a killed Chrome loses recent localStorage writes).
await send("Page.navigate", { url: "about:blank" });
await sleep(1500);
const exited = new Promise((r) => proc.on("exit", r));
send("Browser.close").catch(() => {});
await Promise.race([exited, sleep(8000)]);
proc.kill();
process.exit(0);
