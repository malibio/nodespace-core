/**
 * InvitationsInbox component tests (epic #237, slice S5 #242).
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

vi.mock('$lib/utils/logger', () => ({
	createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const { membershipMock } = vi.hoisted(() => ({
	membershipMock: {
		acceptInvite: vi.fn(() => Promise.resolve('collection-x')),
		requestJoin: vi.fn(() => Promise.resolve('req-1'))
	}
}));
vi.mock('$lib/stores/membership.svelte', () => ({ membership: membershipMock }));

import InvitationsInbox from '$lib/components/collaboration/invitations-inbox.svelte';

describe('InvitationsInbox', () => {
	beforeEach(() => {
		vi.clearAllMocks();
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

	it('requesting to join calls requestJoin and confirms', async () => {
		const { getByPlaceholderText, getByText } = render(InvitationsInbox, {
			props: { open: true, onClose: vi.fn() }
		});
		await fireEvent.input(getByPlaceholderText('collection id'), {
			target: { value: 'col-9' }
		});
		await fireEvent.click(getByText('Request'));
		expect(membershipMock.requestJoin).toHaveBeenCalledWith('col-9');
		expect(getByText(/Request sent/)).toBeTruthy();
		cleanup();
	});

	it('Close calls onClose', async () => {
		const onClose = vi.fn();
		const { getByText } = render(InvitationsInbox, { props: { open: true, onClose } });
		await fireEvent.click(getByText('Close'));
		expect(onClose).toHaveBeenCalled();
		cleanup();
	});
});
