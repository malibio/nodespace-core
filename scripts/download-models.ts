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
const HF_REPO = "nomic-ai/nomic-embed-text-v1.5-GGUF";

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

  // Create models directory
  mkdirSync(MODELS_DIR, { recursive: true });

  // Check if model already exists
  if (existsSync(MODEL_PATH)) {
    console.log("✅ Model already downloaded");
    return;
  }

  // Ensure huggingface-hub is installed (use pip3/python3 for CI/CD compatibility)
  console.log("📦 Ensuring huggingface-hub is installed...");
  await $`pip3 install huggingface-hub`.quiet();

  // Download model using python3 -m to ensure we use the same Python that pip3 installed to
  console.log(`⬇️  Downloading ${MODEL_FILE} from ${HF_REPO}...`);
  await $`python3 -m huggingface_hub.commands.huggingface_cli download ${HF_REPO} ${MODEL_FILE} \
    --local-dir ${MODELS_DIR} \
    --quiet`;

  console.log(`✅ Model downloaded to ${MODEL_PATH}`);

  // Show model size
  const size = await $`du -sh ${MODEL_PATH}`.text();
  console.log(`📊 Model size: ${size.trim()}`);
}

// Run if called directly
if (import.meta.main) {
  await downloadModels();
}
