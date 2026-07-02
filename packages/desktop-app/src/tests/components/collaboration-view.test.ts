/**
 * CollaborationView component tests (epic #237, slice S3 #240).
 * Verifies admin controls gate on the caller's own role and that the community
 * build renders nothing.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
	createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const { membershipMock, proSyncMock } = vi.hoisted(() => ({
	membershipMock: {
		get: vi.fn(),
		currentUserRole: vi.fn(),
		currentPerson: { personId: 'me' } as { personId: string } | null,
		displayFor: (id: string) => id,
		loadCollection: vi.fn(() => Promise.resolve()),
		setMember: vi.fn(() => Promise.resolve()),
		removeMember: vi.fn(() => Promise.resolve()),
		leaveCollection: vi.fn(() => Promise.resolve()),
		createInvite: vi.fn(() => Promise.resolve('CODE123')),
		revokeInvite: vi.fn(() => Promise.resolve()),
		approveRequest: vi.fn(() => Promise.resolve()),
		rejectRequest: vi.fn(() => Promise.resolve())
	},
	proSyncMock: { isPro: true }
}));

vi.mock('$lib/stores/membership.svelte', () => ({ membership: membershipMock }));
vi.mock('$lib/stores/pro-sync.svelte', () => ({ proSync: proSyncMock }));

import CollaborationView from '$lib/components/collaboration/collaboration-view.svelte';

function roster(
	members: Array<{ personId: string; permission: string }>,
	invites: Array<Record<string, string>> = [],
	requests: Array<Record<string, string>> = []
) {
	membershipMock.get.mockReturnValue({
		members,
		invites,
		requests,
		loading: false,
		error: null
	});
}

describe('CollaborationView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		proSyncMock.isPro = true;
		membershipMock.currentPerson = { personId: 'me' };
	});

	it('renders nothing in community mode', () => {
		proSyncMock.isPro = false;
		roster([{ personId: 'me', permission: 'admin' }]);
		const { queryByText } = render(CollaborationView, { props: { collectionId: 'c1' } });
		expect(queryByText('Members')).toBeNull();
		cleanup();
	});

	it('an admin sees role selects, remove, and the add form; self can leave', () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster([
			{ personId: 'me', permission: 'admin' },
			{ personId: 'bob', permission: 'modify' }
		]);
		const { getByLabelText, getByText, queryByLabelText } = render(CollaborationView, {
			props: { collectionId: 'c1' }
		});
		// admin controls on the OTHER member
		expect(getByLabelText('Role for bob')).toBeTruthy();
		expect(getByText('Remove')).toBeTruthy();
		// self row: no role select, a Leave button
		expect(queryByLabelText('Role for me')).toBeNull();
		expect(getByText('Leave')).toBeTruthy();
		// add-existing admin form
		expect(getByText('Add someone already in the workspace')).toBeTruthy();
		cleanup();
	});

	it('a non-admin sees a read-only roster (no selects, no add form)', () => {
		membershipMock.currentUserRole.mockReturnValue('readOnly');
		roster([
			{ personId: 'me', permission: 'readOnly' },
			{ personId: 'bob', permission: 'admin' }
		]);
		const { queryByLabelText, queryByText } = render(CollaborationView, {
			props: { collectionId: 'c1' }
		});
		expect(queryByLabelText('Role for bob')).toBeNull();
		expect(queryByText('Add someone already in the workspace')).toBeNull();
		cleanup();
	});

	it('changing a member role calls setMember', async () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster([
			{ personId: 'me', permission: 'admin' },
			{ personId: 'bob', permission: 'readOnly' }
		]);
		const { getByLabelText } = render(CollaborationView, { props: { collectionId: 'c1' } });
		await fireEvent.change(getByLabelText('Role for bob'), { target: { value: 'modify' } });
		expect(membershipMock.setMember).toHaveBeenCalledWith('c1', 'bob', 'modify');
		cleanup();
	});

	it('loads the collection on mount', () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster([{ personId: 'me', permission: 'admin' }]);
		render(CollaborationView, { props: { collectionId: 'c9' } });
		expect(membershipMock.loadCollection).toHaveBeenCalledWith('c9');
		cleanup();
	});

	// --- S4: invites & requests (admin-only) ---

	it('admin sees the Invites and Join requests sections; non-admin does not', () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster([{ personId: 'me', permission: 'admin' }]);
		const admin = render(CollaborationView, { props: { collectionId: 'c1' } });
		expect(admin.getByText('Create invite')).toBeTruthy();
		expect(admin.getByText('Join requests')).toBeTruthy();
		cleanup();

		membershipMock.currentUserRole.mockReturnValue('readOnly');
		roster([{ personId: 'me', permission: 'readOnly' }]);
		const viewer = render(CollaborationView, { props: { collectionId: 'c1' } });
		expect(viewer.queryByText('Create invite')).toBeNull();
		expect(viewer.queryByText('Join requests')).toBeNull();
		cleanup();
	});

	it('creating a bearer invite calls createInvite (default role/ttl) and shows the code', async () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster([{ personId: 'me', permission: 'admin' }]);
		const { getByText } = render(CollaborationView, { props: { collectionId: 'c1' } });
		await fireEvent.click(getByText('Create invite'));
		// default role readOnly, default ttl 7 days (604800), no email ⇒ undefined
		expect(membershipMock.createInvite).toHaveBeenCalledWith('c1', 'readOnly', undefined, 604800);
		expect(getByText('CODE123')).toBeTruthy(); // bearer code surfaced to copy
		cleanup();
	});

	it('revoking a pending invite calls revokeInvite', async () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster(
			[{ personId: 'me', permission: 'admin' }],
			[{ id: 'i1', code: 'abc', email: '', permission: 'modify', expiresAt: '' }]
		);
		const { getByText } = render(CollaborationView, { props: { collectionId: 'c1' } });
		await fireEvent.click(getByText('Revoke'));
		expect(membershipMock.revokeInvite).toHaveBeenCalledWith('c1', 'i1');
		cleanup();
	});

	it('approving a request uses the selected role; rejecting calls rejectRequest', async () => {
		membershipMock.currentUserRole.mockReturnValue('admin');
		roster(
			[{ personId: 'me', permission: 'admin' }],
			[],
			[{ id: 'r1', requestedBy: 'bob', createdAt: '2026-07-02T00:00:00Z' }]
		);
		const { getByText, getByLabelText } = render(CollaborationView, {
			props: { collectionId: 'c1' }
		});
		await fireEvent.change(getByLabelText('Approve role for bob'), {
			target: { value: 'modify' }
		});
		await fireEvent.click(getByText('Approve'));
		expect(membershipMock.approveRequest).toHaveBeenCalledWith('c1', 'r1', 'modify');

		await fireEvent.click(getByText('Reject'));
		expect(membershipMock.rejectRequest).toHaveBeenCalledWith('c1', 'r1');
		cleanup();
	});
});
