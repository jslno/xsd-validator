import test from "ava";

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { XsdValidator } from "../index.js";
import { readFile } from "node:fs/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));

const assets = [
  "XRECHNUNG_Betriebskostenabrechnung.xml",
  "XRECHNUNG_Einfach.xml",
  "XRECHNUNG_Elektron.xml",
  "XRECHNUNG_Reisekostenabrechnung.xml",
];

const schemaDir = join(__dirname, "./schemas");
const xsd = await readFile(join(schemaDir, "CrossIndustryInvoice_100pD22B.xsd"), "utf-8")
const loader = async (filename: string) => {
  return await readFile(join(schemaDir, filename), "utf-8");
}

for (const asset of assets) {
  test(`validates ${asset}`, async (t) => {
    const xml = await readFile(join(__dirname, "./assets", asset), "utf-8");
    const validator = await XsdValidator.create(xsd, loader);

    t.true(validator.validateOrThrow(xml));
  });
}
