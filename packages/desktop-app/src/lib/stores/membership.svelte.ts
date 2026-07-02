/**
 * Membership store (collection-membership-UI epic #237, slice S1 #238).
 *
 * Holds a per-collection cache of the roster + pending invites/requests and the
 * caller's own identity, so the Collaboration view (S3/S4) and onboarding inbox
 * (S5) read reactive state instead of calling the daemon on every render.
 *
 * Pro-gated: every method early-returns / yields empty in community mode
 * (`proSync.tier !== 'pro'`), so the community build stays behaviorally
 * unchanged — it never touches the membership commands.
 *
 * Caller identity: fetched once via `pro_current_person` (see the S2 design note
 * on #239 — a one-shot query rather than a stream field) and cached; the derived
 * `currentUserRole(collectionId)` matches the caller's own person id against the
 * roster so admin controls gate correctly. Call {@link MembershipStore.reset} on
 * sign-out to clear it (identity is per-session).
 */
import {
	membershipService,
	type Invite,
	type JoinRequest,
	type Member,
	type Permission,
	type Person
} from '$lib/services/membership-service';
import { proSync } from '$lib/stores/pro-sync.svelte';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('MembershipStore');

/** Cached membership state for one collection. */
export interface CollectionMembership {
	members: Member[];
	/** Pending invites — populated only when the caller is an admin, else `[]`. */
	invites: Invite[];
	/** Pending join requests — populated only when the caller is an admin, else `[]`. */
	requests: JoinRequest[];
	loading: boolean;
	error: string | null;
}

function emptyMembership(): CollectionMembership {
	return { members: [], invites: [], requests: [], loading: false, error: null };
}

class MembershipStore {
	/** Per-collection cache, keyed by collection id. Reassigned immutably so runes react. */
	private byCollection = $state<Record<string, CollectionMembership>>({});

	/** The caller's own identity, fetched lazily and cached (cleared on {@link reset}). */
	currentPerson = $state<Person | null>(null);

	/** Pro-gate — the whole store is inert in community mode. */
	get isPro(): boolean {
		return proSync.isPro;
	}

	/**
	 * Guard for the value-returning mutations: throws in community mode so they
	 * never reach the (Pro-only) `pro_*` commands. The void-returning mutations
	 * early-`return` instead. Keeps the store's community-inert contract whole even
	 * though today's only callers are already Pro-gated.
	 */
	private requirePro(): void {
		if (!this.isPro) throw new Error('Pro sync is required for membership operations');
	}

	/** Snapshot for a collection (empty, non-null, when not yet loaded). */
	get(collectionId: string): CollectionMembership {
		return this.byCollection[collectionId] ?? emptyMembership();
	}

	private patch(collectionId: string, next: Partial<CollectionMembership>): void {
		const prev = this.byCollection[collectionId] ?? emptyMembership();
		this.byCollection = { ...this.byCollection, [collectionId]: { ...prev, ...next } };
	}

	/**
	 * The caller's tier in a collection, or `null` if they're not a member / the
	 * roster isn't loaded / identity is unknown. Drives admin-control gating.
	 */
	currentUserRole(collectionId: string): Permission | null {
		const me = this.currentPerson?.personId;
		if (!me) return null;
		const mine = this.get(collectionId).members.find((m) => m.personId === me);
		return mine?.permission ?? null;
	}

	/** True when the caller is an admin of the collection. */
	isAdmin(collectionId: string): boolean {
		return this.currentUserRole(collectionId) === 'admin';
	}

	/**
	 * Display label for a person id. S1 returns the id itself; S3 resolves it to a
	 * name/email from the locally-synced person nodes (email visibility is
	 * `can_see_person`-gated server-side, so a non-co-member legitimately sees id
	 * only). Kept here so the view layer has a single call site to upgrade.
	 */
	displayFor(personId: string): string {
		return personId;
	}

	/**
	 * Ensure the caller's identity is loaded. No-op in community mode, or once a
	 * *bound* identity is cached. Keeps retrying while `personId` is empty so a
	 * mid-session identity bind (device signs in after the store first ran) is
	 * picked up on the next load rather than leaving `currentUserRole` null all
	 * session.
	 */
	async ensureIdentity(): Promise<void> {
		if (!this.isPro || this.currentPerson?.personId) return;
		try {
			this.currentPerson = await membershipService.currentPerson();
		} catch (e) {
			log.warn('currentPerson failed', { error: e });
		}
	}

	/**
	 * Load (or refresh) a collection's membership. Fetches the roster + caller
	 * identity, then — only if the caller is an admin — the pending invites and
	 * join requests (those RPCs are admin-gated server-side, so a non-admin call
	 * would just 403). No-op in community mode.
	 */
	async loadCollection(collectionId: string): Promise<void> {
		if (!this.isPro) return;
		this.patch(collectionId, { loading: true, error: null });
		try {
			await this.ensureIdentity();
			const members = await membershipService.listMembers(collectionId);
			// Determine admin-ness from the freshly-loaded roster (not stale cache).
			const me = this.currentPerson?.personId;
			const amAdmin = !!me && members.some((m) => m.personId === me && m.permission === 'admin');
			// Commit the roster immediately so a failure of the admin-only listings
			// below doesn't discard an already-fetched roster (patch merges).
			this.patch(collectionId, { members, loading: false, error: null });
			if (amAdmin) {
				const [invites, requests] = await Promise.all([
					membershipService.listInvites(collectionId),
					membershipService.listRequests(collectionId)
				]);
				this.patch(collectionId, { invites, requests });
			}
		} catch (e) {
			log.warn('loadCollection failed', { collectionId, error: e });
			this.patch(collectionId, { loading: false, error: String(e) });
		}
	}

	// --- Mutations. Each forwards to the service (server-gated) then refreshes the
	//     affected collection so the cached roster/invites/requests stay in sync. ---

	async setMember(collectionId: string, personId: string, permission: Permission): Promise<void> {
		if (!this.isPro) return;
		await membershipService.setMember(collectionId, personId, permission);
		await this.loadCollection(collectionId);
	}

	async removeMember(collectionId: string, personId: string): Promise<void> {
		if (!this.isPro) return;
		await membershipService.removeMember(collectionId, personId);
		await this.loadCollection(collectionId);
	}

	async leaveCollection(collectionId: string): Promise<void> {
		if (!this.isPro) return;
		await membershipService.leaveCollection(collectionId);
		// After leaving, the caller can no longer read the roster — drop the cache entry.
		const next = { ...this.byCollection };
		delete next[collectionId];
		this.byCollection = next;
	}

	async createInvite(
		collectionId: string,
		permission: Permission,
		email?: string,
		ttlSecs?: number
	): Promise<string> {
		this.requirePro();
		const code = await membershipService.createInvite(collectionId, permission, email, ttlSecs);
		await this.loadCollection(collectionId);
		return code;
	}

	async revokeInvite(collectionId: string, inviteId: string): Promise<void> {
		if (!this.isPro) return;
		await membershipService.revokeInvite(inviteId);
		await this.loadCollection(collectionId);
	}

	async approveRequest(
		collectionId: string,
		requestId: string,
		permission?: Permission
	): Promise<void> {
		if (!this.isPro) return;
		await membershipService.approveRequest(requestId, permission);
		await this.loadCollection(collectionId);
	}

	async rejectRequest(collectionId: string, requestId: string): Promise<void> {
		if (!this.isPro) return;
		// Rejecting a request reuses revoke_invite (same collection_invites row).
		await membershipService.revokeInvite(requestId);
		await this.loadCollection(collectionId);
	}

	/**
	 * Redeem an invite code (onboarding, S5). Returns the joined collection id so
	 * the caller can navigate/pull it. Does not touch the cache (the joined
	 * collection is loaded on demand by its view).
	 */
	async acceptInvite(code: string): Promise<string> {
		this.requirePro();
		return membershipService.acceptInvite(code);
	}

	/** Ask to join a restricted collection (onboarding, S5). Returns the request id. */
	async requestJoin(collectionId: string): Promise<string> {
		this.requirePro();
		return membershipService.requestJoin(collectionId);
	}

	/** Clear all cached state + identity. Call on sign-out (identity is per-session). */
	reset(): void {
		this.byCollection = {};
		this.currentPerson = null;
	}
}

/** Shared singleton — import this, not the class. */
export const membership = new MembershipStore();
