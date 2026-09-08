/**
 * Ai-chat title display and rename-commit logic.
 *
 * An ai-chat node's title lives in its `content` field (the schema declares no
 * `title`/`name` field of its own). Two surfaces show that same value — the
 * viewer header (`ai-chat-node-viewer.svelte`) and the sidebar's chat list
 * (`navigation-sidebar.svelte`) — and both must agree on what "no title yet"
 * reads as, so the fallback lives here once rather than as two copies that
 * could drift.
 */

/**
 * Placeholder shown wherever an ai-chat node's `content` is empty or
 * whitespace-only — a fresh chat before the user renames it, or before
 * background titling (once it lands) fills it in.
 */
export const UNTITLED_CHAT_LABEL = 'Untitled chat';

/** Display title for an ai-chat node's stored `content`. */
export function aiChatDisplayTitle(content: string | null | undefined): string {
  return content?.trim() ? content : UNTITLED_CHAT_LABEL;
}

/**
 * What to persist when a title edit commits, or `null` when nothing should be
 * written.
 *
 * `null` covers both "the user didn't change anything" and "the trimmed
 * result is identical to what's already stored" — writing an unchanged value
 * back would be a no-op mutation that still bumps `modifiedAt` and re-runs
 * the backend's mention extraction for nothing.
 */
export function resolveChatTitleCommit(currentContent: string, draft: string): string | null {
  const trimmed = draft.trim();
  return trimmed === currentContent ? null : trimmed;
}
