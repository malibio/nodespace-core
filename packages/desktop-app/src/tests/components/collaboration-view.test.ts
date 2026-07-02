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
		leaveCollection: vi.fn(() => Promise.resolve())
	},
	proSyncMock: { isPro: true }
}));

vi.mock('$lib/stores/membership.svelte', () => ({ membership: membershipMock }));
vi.mock('$lib/stores/pro-sync.svelte', () => ({ proSync: proSyncMock }));

import CollaborationView from '$lib/components/collaboration/collaboration-view.svelte';

function roster(members: Array<{ personId: string; permission: string }>) {
	membershipMock.get.mockReturnValue({
		members,
		invites: [],
		requests: [],
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
});
