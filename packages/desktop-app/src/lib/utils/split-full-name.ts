/**
 * Best-effort split of a single "full name" suggestion (from git config or the OS
 * account) into first/last name prefill values. Only used for prefilling the
 * onboarding identity form — never written anywhere on its own, and the user can
 * freely edit either field before saving.
 *
 * Splits on the first whitespace: everything before is first_name, everything
 * after (if any) is last_name. A single-word name (or a name with no last part)
 * yields an empty last_name rather than guessing.
 */
export function splitFullName(fullName: string): { firstName: string; lastName: string } {
  const trimmed = fullName.trim();
  if (!trimmed) return { firstName: '', lastName: '' };
  const spaceIndex = trimmed.indexOf(' ');
  if (spaceIndex === -1) return { firstName: trimmed, lastName: '' };
  return {
    firstName: trimmed.slice(0, spaceIndex).trim(),
    lastName: trimmed.slice(spaceIndex + 1).trim()
  };
}
