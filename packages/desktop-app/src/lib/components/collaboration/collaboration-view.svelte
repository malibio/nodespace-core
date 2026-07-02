<!--
  CollaborationView — per-collection membership management (epic #237, slice S3
  #240). Shows the roster (people + role) and, for admins, controls to change a
  member's role, remove them (last-admin protected server-side), and add someone
  already in the workspace. Self-service "Leave" for the caller. Non-admins see a
  read-only roster.

  Pro-only: rendered only when proSync.tier === 'pro' (the host tab is gated too),
  so the community build never mounts this. Invites & join requests (the primary
  onboarding paths) live in the sibling InvitesPanel (S4).
-->
<script lang="ts">
	import { membership } from '$lib/stores/membership.svelte';
	import type { Permission } from '$lib/services/membership-service';
	import { proSync } from '$lib/stores/pro-sync.svelte';
	import { createLogger } from '$lib/utils/logger';

	const log = createLogger('CollaborationView');

	let { collectionId }: { collectionId: string } = $props();

	// readOnly → modify → admin, in ascending-privilege order for the role select.
	const PERMISSIONS: Permission[] = ['readOnly', 'modify', 'admin'];
	const ROLE_LABEL: Record<Permission, string> = {
		readOnly: 'Viewer',
		modify: 'Editor',
		admin: 'Admin'
	};

	let actionError = $state<string | null>(null);
	let busyPerson = $state<string | null>(null);
	let pendingRemove = $state<string | null>(null);
	// Add-existing-person form. A raw person-id input for now — a proper
	// autocomplete picker over locally-synced person nodes is a follow-up once a
	// "list persons" source exists; invites (S4) are the primary way to bring in
	// people who aren't already in the workspace.
	let addId = $state('');
	let addRole = $state<Permission>('readOnly');
	let adding = $state(false);

	$effect(() => {
		if (proSync.isPro && collectionId) {
			membership.loadCollection(collectionId);
		}
	});

	let view = $derived(membership.get(collectionId));
	let myRole = $derived(membership.currentUserRole(collectionId));
	let amAdmin = $derived(myRole === 'admin');
	let myId = $derived(membership.currentPerson?.personId ?? '');

	function friendly(e: unknown): string {
		const s = String(e);
		// Surface the last-admin protection clearly (daemon FAILED_PRECONDITION).
		if (/last[ _]?admin|only admin|last remaining admin/i.test(s)) {
			return 'You can’t remove or demote the last admin of this collection.';
		}
		return s.replace(/^Error:\s*/, '');
	}

	async function changeRole(personId: string, permission: Permission) {
		actionError = null;
		busyPerson = personId;
		try {
			await membership.setMember(collectionId, personId, permission);
		} catch (e) {
			log.warn('changeRole failed', { personId, error: e });
			actionError = friendly(e);
		} finally {
			busyPerson = null;
		}
	}

	async function confirmRemove(personId: string) {
		actionError = null;
		busyPerson = personId;
		try {
			await membership.removeMember(collectionId, personId);
			pendingRemove = null;
		} catch (e) {
			log.warn('removeMember failed', { personId, error: e });
			actionError = friendly(e);
		} finally {
			busyPerson = null;
		}
	}

	async function leave() {
		actionError = null;
		busyPerson = myId;
		try {
			await membership.leaveCollection(collectionId);
		} catch (e) {
			log.warn('leaveCollection failed', { error: e });
			actionError = friendly(e);
		} finally {
			busyPerson = null;
		}
	}

	async function addExisting() {
		const id = addId.trim();
		if (!id) return;
		actionError = null;
		adding = true;
		try {
			await membership.setMember(collectionId, id, addRole);
			addId = '';
		} catch (e) {
			log.warn('addExisting failed', { error: e });
			actionError = friendly(e);
		} finally {
			adding = false;
		}
	}
</script>

{#if proSync.isPro}
	<div class="collaboration">
		{#if view.loading && view.members.length === 0}
			<div class="state-row">Loading members…</div>
		{:else if view.error && view.members.length === 0}
			<div class="state-row error">{friendly(view.error)}</div>
		{:else}
			<div class="section-head">
				<h2>Members</h2>
				<span class="count">{view.members.length}</span>
			</div>

			{#if actionError}
				<div class="state-row error" role="alert">{actionError}</div>
			{/if}

			<ul class="roster">
				{#each view.members as m (m.personId)}
					{@const isSelf = m.personId === myId}
					<li class="member">
						<span class="member-name" title={m.personId}>
							{membership.displayFor(m.personId)}{#if isSelf}<span class="you"> (you)</span>{/if}
						</span>

						{#if amAdmin && !isSelf}
							<select
								class="role-select"
								value={m.permission}
								disabled={busyPerson === m.personId}
								onchange={(e) => changeRole(m.personId, e.currentTarget.value as Permission)}
								aria-label="Role for {m.personId}"
							>
								{#each PERMISSIONS as p}
									<option value={p}>{ROLE_LABEL[p]}</option>
								{/each}
							</select>

							{#if pendingRemove === m.personId}
								<button
									class="btn btn-danger"
									disabled={busyPerson === m.personId}
									onclick={() => confirmRemove(m.personId)}>Confirm remove</button
								>
								<button class="btn btn-ghost" onclick={() => (pendingRemove = null)}>Cancel</button>
							{:else}
								<button class="btn btn-ghost" onclick={() => (pendingRemove = m.personId)}
									>Remove</button
								>
							{/if}
						{:else}
							<span class="role-badge" data-role={m.permission}>{ROLE_LABEL[m.permission]}</span>
							{#if isSelf}
								<button class="btn btn-ghost" disabled={busyPerson === myId} onclick={leave}
									>Leave</button
								>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>

			{#if amAdmin}
				<div class="add-existing">
					<h3>Add someone already in the workspace</h3>
					<div class="add-row">
						<input
							class="add-input"
							type="text"
							placeholder="person node id"
							bind:value={addId}
							disabled={adding}
						/>
						<select class="role-select" bind:value={addRole} disabled={adding} aria-label="Role">
							{#each PERMISSIONS as p}
								<option value={p}>{ROLE_LABEL[p]}</option>
							{/each}
						</select>
						<button class="btn btn-primary" disabled={adding || !addId.trim()} onclick={addExisting}>
							{adding ? 'Adding…' : 'Add'}
						</button>
					</div>
					<p class="hint">To bring in someone new, use <strong>Invites</strong> instead.</p>
				</div>
			{/if}
		{/if}
	</div>
{/if}

<style>
	.collaboration {
		padding: 1.25rem 2rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.section-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.section-head h2 {
		font-size: 1rem;
		font-weight: 600;
		margin: 0;
		color: hsl(var(--foreground));
	}
	.count {
		font-size: 0.8rem;
		color: hsl(var(--muted-foreground));
		padding: 0.1rem 0.5rem;
		background: hsl(var(--muted));
		border-radius: 9999px;
	}
	.state-row {
		font-size: 0.875rem;
		color: hsl(var(--muted-foreground));
	}
	.state-row.error {
		color: hsl(var(--destructive));
	}
	.roster {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.member {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0.5rem;
		border-radius: 6px;
	}
	.member:hover {
		background: hsl(var(--muted) / 0.5);
	}
	.member-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: hsl(var(--foreground));
		font-size: 0.9rem;
	}
	.you {
		color: hsl(var(--muted-foreground));
	}
	.role-badge {
		font-size: 0.75rem;
		padding: 0.1rem 0.5rem;
		border-radius: 9999px;
		background: hsl(var(--muted));
		color: hsl(var(--muted-foreground));
	}
	.role-badge[data-role='admin'] {
		background: hsl(var(--primary) / 0.15);
		color: hsl(var(--primary));
	}
	.role-select,
	.add-input {
		font-size: 0.85rem;
		padding: 0.25rem 0.4rem;
		border: 1px solid hsl(var(--border));
		border-radius: 6px;
		background: hsl(var(--background));
		color: hsl(var(--foreground));
	}
	.add-input {
		flex: 1;
		min-width: 0;
	}
	.btn {
		font-size: 0.8rem;
		padding: 0.25rem 0.6rem;
		border-radius: 6px;
		border: 1px solid transparent;
		cursor: pointer;
	}
	.btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.btn-ghost {
		background: transparent;
		border-color: hsl(var(--border));
		color: hsl(var(--foreground));
	}
	.btn-danger {
		background: hsl(var(--destructive));
		color: #fff;
	}
	.btn-primary {
		background: hsl(var(--primary));
		color: hsl(var(--primary-foreground));
	}
	.add-existing {
		margin-top: 0.75rem;
		border-top: 1px solid hsl(var(--border));
		padding-top: 0.75rem;
	}
	.add-existing h3 {
		font-size: 0.85rem;
		font-weight: 600;
		margin: 0 0 0.5rem;
		color: hsl(var(--foreground));
	}
	.add-row {
		display: flex;
		gap: 0.4rem;
		align-items: center;
	}
	.hint {
		font-size: 0.78rem;
		color: hsl(var(--muted-foreground));
		margin: 0.4rem 0 0;
	}
</style>
