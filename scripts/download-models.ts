#!/usr/bin/env bun
/**
 * Download ML models for development or bundling with the application
 *
 * Usage:
 *   bun run download:models          # Download to ~/.nodespace/models/ (development)
 *   bun run download:models --bundle # Download to resources/ (CI/CD build)
 */

import { $ } from "bun";
import { existsSync, mkdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

const MODEL_FILE = "nomic-embed-text-v1.5.Q8_0.gguf";
const MODEL_URL = `https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/${MODEL_FILE}`;

// Determine target directory based on --bundle flag
const isBundleMode = process.argv.includes("--bundle");
const MODELS_DIR = isBundleMode
  ? "packages/desktop-app/src-tauri/resources/models"
  : join(homedir(), ".nodespace", "models");
const MODEL_PATH = join(MODELS_DIR, MODEL_FILE);

async function downloadModels() {
  const modeLabel = isBundleMode ? "bundling" : "development";
  console.log(`📦 Downloading embedding models for ${modeLabel}...`);
  console.log(`📁 Target directory: ${MODELS_DIR}`);

  mkdirSync(MODELS_DIR, { recursive: true });

  if (existsSync(MODEL_PATH)) {
    console.log("✅ Model already downloaded");
    return;
  }

  console.log(`⬇️  Downloading ${MODEL_FILE}...`);
  await $`curl -L --progress-bar -o ${MODEL_PATH} ${MODEL_URL}`;

  console.log(`✅ Model downloaded to ${MODEL_PATH}`);

  const size = await $`du -sh ${MODEL_PATH}`.text();
  console.log(`📊 Model size: ${size.trim()}`);
}

// Run if called directly
if (import.meta.main) {
  await downloadModels();
}
