<!--
  PersonSchemaForm - Property form for person nodes

  Provides direct editing of name and email fields stored in
  properties.person.{name,email}. Name is also synced to node content
  so it displays inline.

  Email carries a store-aware `unique` schema rule (ADR-065, case-insensitive,
  ignores empty): on blur, a colliding value surfaces a dismissible "a person
  with this email already exists" suggestion — adopt-existing (navigate to the
  match) or keep-as-new (dismiss, keep editing this node). This is
  suggest-don't-block by design: the field save above is never gated on the
  lookup, and create-anyway always remains possible.
-->

<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Alert, AlertDescription } from '$lib/components/ui/alert';
  import { Button } from '$lib/components/ui/button';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { getNavigationService } from '$lib/services/navigation-service';
  import { createLogger } from '$lib/utils/logger';
  import type { Node } from '$lib/types';
  import RelationshipViewerModal from '$lib/components/relationships/relationship-viewer-modal.svelte';
  import WaypointsIcon from '@lucide/svelte/icons/waypoints';
  import UserRoundSearchIcon from '@lucide/svelte/icons/user-round-search';

  const log = createLogger('PersonSchemaForm');

  // Relationships viewer entry point (issue #1918) — inbound relationships (e.g.
  // tasks assigned to this person) surface here.
  let showRelationships = $state(false);

  let { nodeId }: { nodeId: string } = $props();

  const node = $derived(sharedNodeStore.getNode(nodeId));
  const personProps = $derived(
    (node?.properties?.['person'] as Record<string, unknown> | undefined) ?? {}
  );

  const name = $derived((personProps['name'] as string | undefined) ?? '');
  const email = $derived((personProps['email'] as string | undefined) ?? '');

  // Adopt-existing suggestion state (core#1734 / ADR-065). `duplicateMatch` is
  // the existing person the current email collides with, or null when there is
  // none / the suggestion was dismissed. `checkedForEmail` tracks which value
  // the in-flight (or most recent) lookup was for, so a stale response for a
  // value the user has since changed away from — in EITHER the success or the
  // error path — never clobbers a newer, still-valid result.
  let duplicateMatch = $state<Node | null>(null);
  let checkedForEmail: string | null = null;

  // A component instance can be reused across different person nodes (no
  // `{#key nodeId}` at the call site) — reset the suggestion when the node
  // being edited changes, or a suggestion computed for the PREVIOUS person
  // (a different existing-node id, a different email) would linger on screen
  // and "Use existing" would navigate using a match that no longer applies.
  $effect(() => {
    void nodeId;
    duplicateMatch = null;
    checkedForEmail = null;
  });

  async function updateField(field: 'name' | 'email', value: string) {
    if (!node) return;
    try {
      const updatedProperties = {
        ...node.properties,
        person: { ...personProps, [field]: value }
      };
      // Sync name to node content so it renders inline
      const updatedContent = field === 'name' ? value : node.content;
      await backendAdapter.updateNode(nodeId, node.version, {
        content: updatedContent,
        properties: updatedProperties
      });
    } catch (err) {
      log.error('Failed to update person field', { field, err });
    }
  }

  function handleNameBlur(e: FocusEvent) {
    const value = (e.currentTarget as HTMLInputElement).value;
    if (value !== name) updateField('name', value);
  }

  async function handleEmailBlur(e: FocusEvent) {
    const value = (e.currentTarget as HTMLInputElement).value;
    // Fired concurrently, not sequentially: the duplicate check must not wait
    // for the save to land first, or — since both write and read this node's
    // own email — a check that runs AFTER the save sees two rows holding
    // `value` (this node's own freshly-saved copy, plus any real duplicate),
    // and with no ORDER BY on the lookup, could match itself and hide the
    // real duplicate entirely. `excludeId` below closes this structurally
    // regardless of ordering, but firing both together also means the
    // suggestion isn't held back by an in-flight save.
    const tasks: Promise<unknown>[] = [checkForDuplicate(value)];
    if (value !== email) tasks.push(updateField('email', value));
    await Promise.all(tasks);
  }

  /**
   * Suggest-don't-block uniqueness check (ADR-065): looks up an existing
   * active person with the same (case-insensitive) email, excluding this
   * node itself via `excludeId`. Runs on blur (commit), never on every
   * keystroke — a single indexed lookup, not a per-character scan. Skips the
   * round-trip entirely when re-blurring a value already checked, so tabbing
   * through an unchanged field doesn't re-issue it. Never blocks or reverts
   * the save; a lookup failure is logged and simply surfaces no suggestion.
   */
  async function checkForDuplicate(value: string) {
    if (checkedForEmail === value) return;
    checkedForEmail = value;
    if (!value.trim()) {
      duplicateMatch = null;
      return;
    }
    try {
      const match = await backendAdapter.findDuplicateFor('person', 'email', value, nodeId);
      // Ignore a stale response for an email the user has since changed. The
      // backend already excludes this node via excludeId above — the
      // `match.id !== nodeId` check is a defensive backstop, not the primary
      // exclusion mechanism (an earlier version relied on it alone, which
      // could hide a REAL different duplicate whenever this node's own
      // not-yet-excluded row satisfied the query first).
      if (checkedForEmail !== value) return;
      duplicateMatch = match && match.id !== nodeId ? match : null;
    } catch (err) {
      // Same staleness guard as the success path — a slow, now-superseded
      // request that fails after a newer one has already resolved must not
      // clobber that newer (possibly valid) result.
      if (checkedForEmail !== value) return;
      log.error('Duplicate lookup failed (non-blocking)', { err });
      duplicateMatch = null;
    }
  }

  function dismissDuplicateSuggestion() {
    // "Keep as new" — create-anyway. Nothing to undo: the field save already
    // went through above: this only clears the suggestion banner.
    duplicateMatch = null;
  }

  function adoptExisting() {
    if (!duplicateMatch) return;
    // "Use existing" — open the existing person instead. Deliberately
    // non-destructive: this does not delete or merge the current node (full
    // merge machinery is out of scope for this rule), it just gets the user to
    // the record they meant to use. No sourcePaneId is available from this
    // form's props, so with two panes open this resolves against the current
    // active pane rather than necessarily the pane hosting this form — the
    // same fallback other unparented call sites of this navigation helper
    // already accept.
    getNavigationService().navigateToNodeInOtherPane(duplicateMatch.id);
    duplicateMatch = null;
  }

  const duplicateDisplayName = $derived(
    (duplicateMatch?.properties?.['person'] as Record<string, unknown> | undefined)?.[
      'name'
    ] as string | undefined
  );
</script>

<div class="person-schema-form">
  <div class="field">
    <label for="person-name">Name</label>
    <Input
      id="person-name"
      type="text"
      value={name}
      placeholder="Display name"
      onblur={handleNameBlur}
    />
  </div>
  <div class="field">
    <label for="person-email">Email</label>
    <Input
      id="person-email"
      type="email"
      value={email}
      placeholder="email@example.com"
      onblur={handleEmailBlur}
    />
  </div>

  {#if duplicateMatch}
    <Alert variant="warning">
      <UserRoundSearchIcon class="h-4 w-4" />
      <AlertDescription class="duplicate-message">
        A person with this email already exists{duplicateDisplayName
          ? `: ${duplicateDisplayName}`
          : ''} — use them instead?
      </AlertDescription>
      <!-- AlertDescription renders a <p>, which cannot contain block content
           (another <p>, a <div>) without the browser silently restructuring
           the DOM — so the action buttons are a sibling, not a child. -->
      <div class="duplicate-actions">
        <Button type="button" size="sm" variant="outline" onclick={adoptExisting}>
          Use existing
        </Button>
        <Button type="button" size="sm" variant="ghost" onclick={dismissDuplicateSuggestion}>
          Keep as new
        </Button>
      </div>
    </Alert>
  {/if}

  <!-- Relationships entry point (issue #1918) -->
  <button
    type="button"
    class="flex w-full items-center gap-2 py-2 text-sm font-medium text-muted-foreground transition-all hover:opacity-80"
    onclick={() => (showRelationships = true)}
  >
    <WaypointsIcon class="h-4 w-4" />
    <span>Relationships</span>
  </button>
</div>

<RelationshipViewerModal bind:open={showRelationships} {nodeId} />

<style>
  .person-schema-form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.5rem 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  label {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    font-weight: 500;
  }

  /* `class` on <AlertDescription> is forwarded to that component's own
     internal element rather than applied to one in THIS template, so the
     Svelte compiler can't see the usage and would otherwise warn this
     selector unused. */
  :global(.duplicate-message) {
    margin: 0 0 0.5rem 0;
  }

  .duplicate-actions {
    display: flex;
    gap: 0.5rem;
  }
</style>
