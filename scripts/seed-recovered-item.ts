#!/usr/bin/env bun
/**
 * Seed one entry into the Recovered Items log so the Pro "Recovered Items" UI
 * can be smoke-tested without staging a real two-device LWW conflict.
 *
 * The Pro daemon writes superseded conflict-losers to
 *   ~/.nodespace/recovered-items-<user>.jsonl   (snake_case, one JSON object/line)
 * and the desktop app reads that same file for the current user. This helper
 * appends a synthetic entry for a REAL node id so the badge attaches on relaunch.
 *
 * Usage:
 *   bun run scripts/seed-recovered-item.ts [--user <u>] [--db <path>] [--node <id>] \
 *                                          [--mine <text>] [--won <text>]
 *
 *   --user  recovery-log user (default: "default" — the bundled desktop daemon;
 *           use "demo-a"/"demo-b" for the two-window demo). Picks the log file:
 *           ~/.nodespace/recovered-items-<user>.jsonl
 *   --db    libsql/sqlite DB to pick a real node from when --node is omitted
 *           (default: ~/.nodespace/database/nodespace.db;
 *            two-window demo: /tmp/ns-demo-a/db)
 *   --node  node id to attach the badge to (default: auto-pick a text node)
 *   --mine  superseded ("your") content   (default: "my offline edit")
 *   --won   winning ("current") content   (default: "the edit that won")
 *
 * After seeding: ⌘Q the app and relaunch → snackbar + ⟲ badge on that node.
 */

import { Database } from "bun:sqlite";
import { appendFileSync, existsSync, mkdirSync } from "fs";
import { join } from "path";
import { homedir } from "os";

interface Args {
  user: string;
  db: string;
  node: string;
  mine: string;
  won: string;
}

function parseArgs(argv: string[]): Args {
  const args: Args = {
    user: "default",
    db: join(homedir(), ".nodespace", "database", "nodespace.db"),
    node: "",
    mine: "my offline edit",
    won: "the edit that won",
  };

  for (let i = 0; i < argv.length; i++) {
    const next = () => {
      const v = argv[++i];
      if (v === undefined) {
        console.error(`missing value for ${argv[i - 1]}`);
        process.exit(2);
      }
      return v;
    };
    switch (argv[i]) {
      case "--user":
        args.user = next();
        break;
      case "--db":
        args.db = next();
        break;
      case "--node":
        args.node = next();
        break;
      case "--mine":
        args.mine = next();
        break;
      case "--won":
        args.won = next();
        break;
      case "--help":
      case "-h":
        printUsage();
        process.exit(0);
        break;
      default:
        console.error(`unknown arg: ${argv[i]}`);
        process.exit(2);
    }
  }

  return args;
}

function printUsage() {
  console.log(
    'Usage: bun run scripts/seed-recovered-item.ts [--user <u>] [--db <path>] [--node <id>] [--mine <text>] [--won <text>]',
  );
}

/** Format a Date as RFC3339 UTC without milliseconds, e.g. 2026-07-28T12:00:00+00:00. */
function rfc3339(d: Date): string {
  return d.toISOString().replace(/\.\d{3}Z$/, "+00:00");
}

/** Auto-pick the most recently modified non-empty text node from the DB. */
function pickNode(dbPath: string): string {
  if (!existsSync(dbPath)) {
    console.error(`DB not found: ${dbPath} (pass --db)`);
    process.exit(1);
  }
  const db = new Database(dbPath, { readonly: true });
  try {
    const row = db
      .query(
        "SELECT id FROM node WHERE node_type='text' AND content<>'' ORDER BY modified_at DESC LIMIT 1;",
      )
      .get() as { id: string } | null;
    if (!row?.id) {
      console.error(`no text node found in ${dbPath} — pass --node <id>`);
      process.exit(1);
    }
    return row.id;
  } finally {
    db.close();
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2));

  const nsDir = join(homedir(), ".nodespace");
  const log = join(nsDir, `recovered-items-${args.user}.jsonl`);
  mkdirSync(nsDir, { recursive: true });

  let node = args.node;
  if (!node) {
    node = pickNode(args.db);
    console.log(`auto-picked node ${node} from ${args.db}`);
  }

  // Timestamps: superseded (oldest) < winning < recovered (now). RFC3339 UTC.
  const now = Date.now();
  const supAt = rfc3339(new Date(now - 60 * 60 * 1000)); // -1h
  const winAt = rfc3339(new Date(now - 30 * 60 * 1000)); // -30m
  const recAt = rfc3339(new Date(now));

  const line = JSON.stringify({
    node_id: node,
    superseded_content: args.mine,
    superseded_modified_at: supAt,
    winning_content: args.won,
    winning_modified_at: winAt,
    recovered_at: recAt,
  });

  appendFileSync(log, line + "\n");

  console.log(`✓ seeded → ${log}`);
  console.log(`  node:       ${node}`);
  console.log(`  superseded: ${args.mine}  (${supAt})`);
  console.log(`  winning:    ${args.won}  (${winAt})`);
  console.log();
  console.log(
    `Now ⌘Q the app and relaunch (user=${args.user}). Expect: snackbar once + ⟲ badge on that node.`,
  );
}

if (import.meta.main) {
  main();
}
