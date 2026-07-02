#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = resolve(scriptPath, "../..");

function readJson(relativePath) {
  return JSON.parse(readFileSync(resolve(repoRoot, relativePath), "utf8"));
}

function readText(relativePath) {
  return readFileSync(resolve(repoRoot, relativePath), "utf8");
}

function parseCargoPackageVersion(source) {
  const match = source.match(/^\[package\][\s\S]*?^\s*version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("Cannot find [package].version in src-tauri/Cargo.toml.");
  }
  return match[1].trim();
}

function main() {
  const packageJson = readJson("package.json");
  const tauriConfig = readJson("src-tauri/tauri.conf.json");
  const cargoToml = readText("src-tauri/Cargo.toml");

  const packageName = String(packageJson.name ?? "").trim();
  const packageVersion = String(packageJson.version ?? "").trim();
  const tauriVersion = String(tauriConfig.version ?? "").trim();
  const cargoVersion = parseCargoPackageVersion(cargoToml);

  if (!packageName) {
    throw new Error("package.json is missing name.");
  }
  if (!packageVersion) {
    throw new Error("package.json is missing version.");
  }

  const mismatches = [
    ["package.json", packageVersion],
    ["src-tauri/Cargo.toml", cargoVersion],
    ["src-tauri/tauri.conf.json", tauriVersion],
  ].filter(([, version]) => version !== packageVersion);

  if (mismatches.length > 0) {
    const details = mismatches.map(([file, version]) => `${file}=${version}`).join(", ");
    throw new Error(
      `Release version mismatch. package.json=${packageVersion}; ${details}. Keep all release versions in sync before publishing.`
    );
  }

  const tagName = `${packageName}-v${packageVersion}`;
  process.stdout.write(`package_name=${packageName}\n`);
  process.stdout.write(`version=${packageVersion}\n`);
  process.stdout.write(`tag_name=${tagName}\n`);
}

main();
