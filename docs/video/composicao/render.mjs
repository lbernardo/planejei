// Renderiza index.html quadro a quadro no Chrome e junta com a narração no ffmpeg.
//
//   yarn render                       → ../planejei-apresentacao.mp4
//   yarn render --from 30 --to 40     → trecho, para conferir
//   yarn render --stills 5,20,45      → só PNGs desses instantes em ./stills
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import puppeteer from "puppeteer-core";

const here = dirname(fileURLToPath(import.meta.url));
const arg = (name, def) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : def;
};
const FPS = 30;
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const AUDIO = join(here, "..", "audio", "narracao.mp3");

const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: ["--hide-scrollbars", "--allow-file-access-from-files", "--force-color-profile=srgb"],
});
const page = await browser.newPage();
await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
await page.goto(pathToFileURL(join(here, "index.html")).href + "?render=1", { waitUntil: "load" });
await page.evaluate(() => Promise.all([...document.images].map(i => i.decode().catch(() => {}))));
const duration = await page.evaluate(() => window.DURATION);

const stills = arg("stills");
if (stills) {
  mkdirSync(join(here, "stills"), { recursive: true });
  for (const s of stills.split(",").map(Number)) {
    await page.evaluate(t => window.render(t), s);
    await page.screenshot({ path: join(here, "stills", `t${s.toFixed(1)}.png`) });
  }
  await browser.close();
  process.exit(0);
}

const from = Number(arg("from", 0));
const to = Math.min(Number(arg("to", duration)), duration);
const out = arg("out", join(here, "..", from === 0 && to === duration ? "planejei-apresentacao.mp4" : "trecho.mp4"));
const frames = Math.round((to - from) * FPS);

// A narração começa em OFF no vídeo; no trecho, o áudio é deslocado junto.
const off = await page.evaluate(() => 0.4);
const ff = spawn("ffmpeg", [
  "-y", "-hide_banner", "-loglevel", "error",
  "-f", "image2pipe", "-framerate", String(FPS), "-c:v", "mjpeg", "-i", "-",
  "-i", AUDIO,
  "-filter_complex",
  `[1:a]adelay=${Math.round(off * 1000)}:all=1,atrim=start=${from},asetpts=PTS-STARTPTS,apad[a]`,
  "-map", "0:v", "-map", "[a]", "-t", String(to - from),
  "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
  "-c:a", "aac", "-b:a", "192k",
  out,
], { stdio: ["pipe", "inherit", "inherit"] });

const t0 = Date.now();
for (let f = 0; f < frames; f++) {
  const t = from + f / FPS;
  await page.evaluate(t => window.render(t), t);
  const buf = await page.screenshot({ type: "jpeg", quality: 95, optimizeForSpeed: true });
  if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once("drain", r));
  if (f % 150 === 0) process.stdout.write(`\r${(t).toFixed(1)} / ${to.toFixed(1)} s  (${((Date.now() - t0) / 1000).toFixed(0)} s)`);
}
ff.stdin.end();
await new Promise(r => ff.on("close", r));
await browser.close();
console.log(`\npronto: ${out}`);
