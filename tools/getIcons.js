import { readFile, writeFile } from "node:fs/promises";
import * as path from "node:path";

const dirname = import.meta.dirname;

const iconIds = process.argv[2]
  ? process.argv[2].split(",")
  : (await readFile(path.join(dirname, "icons.txt"), "utf8")).split("\n");

for (let i = 0; i < iconIds.length; i++) {
  const iconId = iconIds[i];
  console.log(`Fetching ${i + 1}/${iconIds.length}: ${iconId}`);
  const url = `https://fonts.gstatic.com/s/i/short-term/release/materialsymbols/${iconId}/wght500/48px.svg`;
  const response = await fetch(url);
  if (!response.ok) {
    console.error("Could not download " + iconId);
    break;
  }
  const blob = await response.blob();
  const arrayBuffer = await blob.arrayBuffer();
  const buffer = Buffer.from(arrayBuffer, "binary");
  const targetFilename = `${iconId}.svg`;
  const targetPath = path.join(
    dirname, "..", "src", "miconau", "static", "icons",
    targetFilename,
  );
  console.log("Writing " + targetFilename);
  await writeFile(targetPath, buffer);
}
