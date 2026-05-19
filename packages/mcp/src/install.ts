#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const CLAUDE_CONFIG_PATH = join(
  homedir(),
  'Library',
  'Application Support',
  'Claude',
  'claude_desktop_config.json'
);

const MCP_SERVER_KEY = 'nodespace';

const MCP_SERVER_ENTRY = {
  command: 'npx',
  args: ['@nodespaceai/mcp']
};

type ClaudeConfig = {
  mcpServers?: Record<string, { command: string; args: string[] }>;
  [key: string]: unknown;
};

function readConfig(): ClaudeConfig {
  if (!existsSync(CLAUDE_CONFIG_PATH)) return {};
  try {
    return JSON.parse(readFileSync(CLAUDE_CONFIG_PATH, 'utf8')) as ClaudeConfig;
  } catch {
    console.error(`Warning: could not parse ${CLAUDE_CONFIG_PATH}, treating as empty.`);
    return {};
  }
}

function writeConfig(config: ClaudeConfig): void {
  const dir = join(homedir(), 'Library', 'Application Support', 'Claude');
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  writeFileSync(CLAUDE_CONFIG_PATH, JSON.stringify(config, null, 2) + '\n', 'utf8');
}

function install(): void {
  const config = readConfig();
  config.mcpServers ??= {};
  config.mcpServers[MCP_SERVER_KEY] = MCP_SERVER_ENTRY;
  writeConfig(config);
  console.log(`✅ NodeSpace MCP server registered in ${CLAUDE_CONFIG_PATH}`);
  console.log('Restart Claude desktop to activate the nodespace tools.');
}

function uninstall(): void {
  const config = readConfig();
  if (!config.mcpServers?.[MCP_SERVER_KEY]) {
    console.log('NodeSpace MCP server was not registered — nothing to do.');
    return;
  }
  delete config.mcpServers[MCP_SERVER_KEY];
  writeConfig(config);
  console.log(`✅ NodeSpace MCP server removed from ${CLAUDE_CONFIG_PATH}`);
}

const command = process.argv[2];
if (command === 'install' || command === 'uninstall') {
  if (process.platform !== 'darwin') {
    console.error(
      `Error: automatic config install is only supported on macOS (detected: ${process.platform}).\n` +
        'Add the following to your Claude desktop config manually:\n\n' +
        JSON.stringify({ mcpServers: { nodespace: MCP_SERVER_ENTRY } }, null, 2)
    );
    process.exit(1);
  }
  if (command === 'install') {
    install();
  } else {
    uninstall();
  }
} else {
  console.error('Usage: nodespace-mcp install | uninstall');
  process.exit(1);
}
