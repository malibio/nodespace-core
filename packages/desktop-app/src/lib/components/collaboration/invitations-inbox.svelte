<!--
  InvitationsInbox — onboarding entry point (epic #237, slice S5 #242). A modal
  opened from the Pro account menu (and once, on first sign-in). Lets a user
  redeem a share-code invite and ask to join a restricted collection.

  Email-bound invites are auto-redeemed server-side on reconnect
  (`redeem_my_invites`), so they need no action here — the copy just says so.
  Listing "my pending invites / my request statuses" and discovering joinable
  collections have no client-facing backend yet (only admins can list a
  collection's invites/requests); those are deferred to a follow-up.
-->
<script lang="ts">
	import { focusTrap } from '$lib/actions/focus-trap';
	import { membership } from '$lib/stores/membership.svelte';
	import { createLogger } from '$lib/utils/logger';

	const log = createLogger('InvitationsInbox');

	let { open = false, onClose }: { open?: boolean; onClose: () => void } = $props();

	let code = $state('');
	let redeeming = $state(false);
	let redeemMsg = $state<string | null>(null);
	let redeemErr = $state<string | null>(null);

	let joinId = $state('');
	let requesting = $state(false);
	let requestMsg = $state<string | null>(null);
	let requestErr = $state<string | null>(null);

	function friendly(e: unknown): string {
		return String(e).replace(/^Error:\s*/, '');
	}

	async function redeem() {
		const c = code.trim();
		if (!c) return;
		redeeming = true;
		redeemMsg = null;
		redeemErr = null;
		try {
			await membership.acceptInvite(c);
			redeemMsg = 'You now have access — it will appear in your sidebar as it syncs.';
			code = '';
		} catch (e) {
			log.warn('redeem failed', { error: e });
			redeemErr = friendly(e);
		} finally {
			redeeming = false;
		}
	}

	async function requestJoin() {
		const id = joinId.trim();
		if (!id) return;
		requesting = true;
		requestMsg = null;
		requestErr = null;
		try {
			await membership.requestJoin(id);
			requestMsg = 'Request sent — an admin will review it.';
			joinId = '';
		} catch (e) {
			log.warn('requestJoin failed', { error: e });
			requestErr = friendly(e);
		} finally {
			requesting = false;
		}
	}
</script>

{#if open}
	<div class="overlay" role="presentation" tabindex="-1" onclick={onClose}>
		<div
			class="modal"
			role="dialog"
			aria-modal="true"
			aria-labelledby="inv-title"
			use:focusTrap={{ onEscape: onClose }}
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.stopPropagation()}
			tabindex="0"
		>
			<h2 id="inv-title">Invitations</h2>
			<p class="lead">
				Email invites apply automatically when you sign in. To join with a share code, or ask to
				join a restricted collection, use the options below.
			</p>

			<section>
				<h3>Redeem an invite code</h3>
				<div class="row">
					<input
						class="in"
						type="text"
						placeholder="paste invite code"
						bind:value={code}
						disabled={redeeming}
					/>
					<button class="btn btn-primary" disabled={redeeming || !code.trim()} onclick={redeem}>
						{redeeming ? 'Redeeming…' : 'Redeem'}
					</button>
				</div>
				{#if redeemMsg}<p class="ok" role="status">{redeemMsg}</p>{/if}
				{#if redeemErr}<p class="err" role="alert">{redeemErr}</p>{/if}
			</section>

			<section>
				<h3>Request to join a collection</h3>
				<div class="row">
					<input
						class="in"
						type="text"
						placeholder="collection id"
						bind:value={joinId}
						disabled={requesting}
					/>
					<button
						class="btn btn-ghost"
						disabled={requesting || !joinId.trim()}
						onclick={requestJoin}
					>
						{requesting ? 'Sending…' : 'Request'}
					</button>
				</div>
				{#if requestMsg}<p class="ok" role="status">{requestMsg}</p>{/if}
				{#if requestErr}<p class="err" role="alert">{requestErr}</p>{/if}
				<!-- Discovering joinable collections (search/browse) is Phase 2; for now
				     the admin shares the collection id out of band. -->
			</section>

			<div class="actions">
				<button class="btn btn-ghost" onclick={onClose}>Close</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}
	.modal {
		background: var(--surface-1, #ffffff);
		color: var(--text-primary, #1f2937);
		border-radius: 8px;
		padding: 24px;
		width: min(460px, calc(100vw - 48px));
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
	}
	h2 {
		margin: 0 0 4px;
		font-size: 1.1rem;
		font-weight: 600;
	}
	.lead {
		margin: 0 0 16px;
		font-size: 0.85rem;
		color: var(--text-secondary, #6b7280);
	}
	section {
		margin-bottom: 16px;
	}
	h3 {
		margin: 0 0 8px;
		font-size: 0.9rem;
		font-weight: 600;
	}
	.row {
		display: flex;
		gap: 8px;
	}
	.in {
		flex: 1;
		min-width: 0;
		padding: 8px 10px;
		font-size: 0.875rem;
		border: 1px solid var(--border-color, #d1d5db);
		border-radius: 6px;
		background: var(--surface-1, #fff);
		color: var(--text-primary, #1f2937);
	}
	.btn {
		padding: 8px 14px;
		border: 1px solid transparent;
		border-radius: 6px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}
	.btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.btn-primary {
		background: #2563eb;
		color: #fff;
	}
	.btn-ghost {
		background: var(--surface-2, #e5e7eb);
		color: var(--text-primary, #1f2937);
	}
	.ok {
		margin: 8px 0 0;
		font-size: 0.8rem;
		color: #16a34a;
	}
	.err {
		margin: 8px 0 0;
		font-size: 0.8rem;
		color: #dc2626;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 8px;
	}
</style>
