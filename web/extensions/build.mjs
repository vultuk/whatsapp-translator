import { build } from "esbuild";
import { readFile, mkdir, writeFile } from "node:fs/promises";
const base = new URL("./", import.meta.url);
const result = await build({
  entryPoints: [new URL("src/app.ts", base).pathname],
  bundle: true,
  write: false,
  format: "iife",
  target: "es2022",
  minify: true,
  legalComments: "none",
});
const styles = await readFile(new URL("src/styles.css", base), "utf8");
const html = (await readFile(new URL("src/chats.html", base), "utf8"))
  .replace("/* EXTENSION_STYLES */", () => styles)
  .replace("/* EXTENSION_SCRIPT */", () =>
    result.outputFiles[0].text.replace(/<\/script/gi, "<\\/script"),
  );
const output = new URL("dist/chats.html", base);
if (process.argv.includes("--check")) {
  if ((await readFile(output, "utf8")) !== html)
    throw Error("Extension bundle is stale. Run npm run build:extension.");
} else {
  await mkdir(new URL("dist/", base), { recursive: true });
  await writeFile(output, html);
}
