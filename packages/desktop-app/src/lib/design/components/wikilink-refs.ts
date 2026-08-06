/**
 * Wikilink reference helpers (pure, DOM-free).
 *
 * Detects bare `[[node-id]]` wikilink tokens inside a plain-text run and splits
 * that run into text/ref segments so the view renderer can turn valid ids into
 * clickable node references. Only ids the backend treats as references become
 * `ref` segments — a UUID or an ISO date (`YYYY-MM-DD`), optionally `node/`
 * prefixed; every other `[[...]]` (titles, `[[TODO]]`, `[[]]`) stays literal
 * text.
 *
 * This mirrors the backend `extract_mentions` wikilink recognition and its
 * `is_valid_node_id` gate (packages/core/src/services/node_service/mod.rs) so
 * what renders as a reference matches what the backend recorded as a mention
 * edge. Render-only: it never mutates stored content.
 */

import { isValidDateId } from '$lib/types/date-node';

// UUID 8-4-4-4-12 hex shape. Lowercase-only, matching the backend's
// is_valid_node_id (and node ids are generated lowercase), so what renders as a
// reference is exactly what the backend recorded a mention edge for — an
// uppercase `[[UUID]]` stays literal text rather than becoming a dead reference.
const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

// `[[<token>]]` with an optional `node/` prefix. The token class excludes `]`
// and whitespace, so `[[some title]]` and `[[ ]]` never match. Mirrors the
// backend WIKILINK_MENTION_PATTERN. Group 1 = optional `node/`, group 2 = id.
const WIKILINK_PATTERN = String.raw`\[\[(node/)?([^\]\s]+)\]\]`;

const NODE_PREFIX = 'node/';

export type RefSegment = { kind: 'text'; value: string } | { kind: 'ref'; id: string };

/**
 * True only for a token that resolves to a real node id: a UUID (any case) or a
 * valid ISO date (`YYYY-MM-DD`). A leading `node/` prefix is stripped first, so
 * `node/<id>` is accepted exactly as the backend accepts it.
 */
export function isValidNodeRefId(token: string): boolean {
  const id = token.startsWith(NODE_PREFIX) ? token.slice(NODE_PREFIX.length) : token;
  // A UUID or a real calendar date (isValidDateId rejects shape-only values like
  // 2025-13-45 and is timezone-safe) — the same two id shapes the backend accepts.
  return UUID_REGEX.test(id) || isValidDateId(id);
}

/**
 * Scan a plain-text string for `[[<token>]]` occurrences and return an ordered
 * list of segments. A `ref` segment is emitted only when the token is a valid
 * node id; otherwise the literal `[[...]]` is preserved as part of the
 * surrounding `text`. All non-matched text is preserved exactly, so joining the
 * segment values reproduces the original string.
 */
export function splitTextIntoRefSegments(text: string): RefSegment[] {
  const segments: RefSegment[] = [];
  if (!text) return segments;

  // Fresh regex per call: a stateful `g` regex would carry lastIndex between
  // invocations and is not safe to share.
  const regex = new RegExp(WIKILINK_PATTERN, 'g');
  let lastEmitted = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(text)) !== null) {
    const id = match[2];

    // Invalid token: leave it literal. Do not advance `lastEmitted`, so the
    // `[[...]]` stays inside the next emitted text slice. `regex.lastIndex` has
    // already advanced past this match, so the loop still makes progress.
    if (!isValidNodeRefId(id)) continue;

    if (match.index > lastEmitted) {
      segments.push({ kind: 'text', value: text.slice(lastEmitted, match.index) });
    }
    segments.push({ kind: 'ref', id });
    lastEmitted = match.index + match[0].length;
  }

  if (lastEmitted < text.length) {
    segments.push({ kind: 'text', value: text.slice(lastEmitted) });
  }

  return segments;
}
