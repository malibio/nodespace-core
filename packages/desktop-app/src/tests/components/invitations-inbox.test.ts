/**
 * InvitationsInbox component tests.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
	createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const { membershipMock } = vi.hoisted(() => ({
	membershipMock: {
		isPro: true,
		joinable: [] as Array<{ id: string; name: string; restricted: boolean }>,
		joinableLoading: false,
		joinableError: null as string | null,
		loadJoinable: vi.fn(() => Promise.resolve()),
		acceptInvite: vi.fn(() => Promise.resolve('collection-x')),
		requestJoin: vi.fn(() => Promise.resolve('req-1')),
		joinCollection: vi.fn(() => Promise.resolve())
	}
}));
vi.mock('$lib/stores/membership.svelte', () => ({ membership: membershipMock }));

import InvitationsInbox from '$lib/components/collaboration/invitations-inbox.svelte';

describe('InvitationsInbox', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		membershipMock.isPro = true;
		membershipMock.joinable = [];
		membershipMock.joinableLoading = false;
		membershipMock.joinableError = null;
	});

	it('renders nothing when closed', () => {
		const { queryByText } = render(InvitationsInbox, {
			props: { open: false, onClose: vi.fn() }
		});
		expect(queryByText('Invitations')).toBeNull();
		cleanup();
	});

	it('redeeming a code calls acceptInvite and shows a success message', async () => {
		const { getByPlaceholderText, getByText } = render(InvitationsInbox, {
			props: { open: true, onClose: vi.fn() }
		});
		await fireEvent.input(getByPlaceholderText('paste invite code'), {
			target: { value: 'CODE42' }
		});
		await fireEvent.click(getByText('Redeem'));
		expect(membershipMock.acceptInvite).toHaveBeenCalledWith('CODE42');
		expect(getByText(/You now have access/)).toBeTruthy();
		cleanup();
	});

	it('a redeem failure surfaces the error', async () => {
		membershipMock.acceptInvite.mockRejectedValueOnce('Error: invalid or expired invite code');
		const { getByPlaceholderText, getByText } = render(InvitationsInbox, {
			props: { open: true, onClose: vi.fn() }
		});
		await fireEvent.input(getByPlaceholderText('paste invite code'), {
			target: { value: 'bad' }
		});
		await fireEvent.click(getByText('Redeem'));
		expect(getByText(/invalid or expired invite code/)).toBeTruthy();
		cleanup();
	});

	it('loads the discovery list on open (Pro)', () => {
		render(InvitationsInbox, { props: { open: true, onClose: vi.fn() } });
		expect(membershipMock.loadJoinable).toHaveBeenCalled();
		cleanup();
	});

	it('shows an empty state when there is nothing to join', () => {
		const { getByText } = render(InvitationsInbox, { props: { open: true, onClose: vi.fn() } });
		expect(getByText(/No collections available to join/)).toBeTruthy();
		cleanup();
	});

	it('requesting a restricted collection from the list calls requestJoin and confirms', async () => {
		membershipMock.joinable = [{ id: 'col-9', name: 'Legal', restricted: true }];
		const { getByText } = render(InvitationsInbox, { props: { open: true, onClose: vi.fn() } });
		expect(getByText('Legal')).toBeTruthy();
		expect(getByText('Restricted')).toBeTruthy();
		await fireEvent.click(getByText('Request'));
		expect(membershipMock.requestJoin).toHaveBeenCalledWith('col-9');
		expect(membershipMock.joinCollection).not.toHaveBeenCalled();
		expect(getByText(/Request sent for Legal/)).toBeTruthy();
		cleanup();
	});

	it('joining an open collection from the list calls joinCollection and confirms', async () => {
		membershipMock.joinable = [{ id: 'col-open', name: 'Marketing', restricted: false }];
		const { getByText } = render(InvitationsInbox, { props: { open: true, onClose: vi.fn() } });
		expect(getByText('Marketing')).toBeTruthy();
		expect(getByText('Open')).toBeTruthy();
		await fireEvent.click(getByText('Join'));
		expect(membershipMock.joinCollection).toHaveBeenCalledWith('col-open');
		expect(membershipMock.requestJoin).not.toHaveBeenCalled();
		expect(getByText(/Joined Marketing/)).toBeTruthy();
		cleanup();
	});

	it('Close calls onClose', async () => {
		const onClose = vi.fn();
		const { getByText } = render(InvitationsInbox, { props: { open: true, onClose } });
		await fireEvent.click(getByText('Close'));
		expect(onClose).toHaveBeenCalled();
		cleanup();
	});

	it('shows a Log out button only when onLogout is provided, and invokes it (#248)', async () => {
		// No onLogout → no logout affordance (existing callers unaffected).
		const { queryByText, unmount } = render(InvitationsInbox, {
			props: { open: true, onClose: vi.fn() }
		});
		expect(queryByText('Log out')).toBeNull();
		unmount();

		// With onLogout → a signed-in user with no code/membership can revert to
		// the free/signed-out state from here.
		const onLogout = vi.fn(() => Promise.resolve());
		const { getByText } = render(InvitationsInbox, {
			props: { open: true, onClose: vi.fn(), onLogout }
		});
		await fireEvent.click(getByText('Log out'));
		expect(onLogout).toHaveBeenCalled();
		cleanup();
	});
});
