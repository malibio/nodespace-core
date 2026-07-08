import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
	createLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() })
}));

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args)
}));

import { membershipService } from '$lib/services/membership-service';

describe('MembershipService', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('listMembers maps snake_case rows to camelCase Member[]', async () => {
		mockInvoke.mockResolvedValue([
			{ person_id: 'p1', permission: 'admin' },
			{ person_id: 'p2', permission: 'readOnly' }
		]);
		const members = await membershipService.listMembers('c1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_list_members', { collectionId: 'c1' });
		expect(members).toEqual([
			{ personId: 'p1', permission: 'admin' },
			{ personId: 'p2', permission: 'readOnly' }
		]);
	});

	it('setMember forwards camelCase args to pro_set_member', async () => {
		mockInvoke.mockResolvedValue(undefined);
		await membershipService.setMember('c1', 'p1', 'modify');
		expect(mockInvoke).toHaveBeenCalledWith('pro_set_member', {
			collectionId: 'c1',
			personId: 'p1',
			permission: 'modify'
		});
	});

	it('removeMember / leaveCollection forward the right command + args', async () => {
		mockInvoke.mockResolvedValue(undefined);
		await membershipService.removeMember('c1', 'p1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_remove_member', {
			collectionId: 'c1',
			personId: 'p1'
		});
		await membershipService.leaveCollection('c1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_leave_collection', { collectionId: 'c1' });
	});

	it('createInvite omits email/ttl when not given, includes them when given', async () => {
		mockInvoke.mockResolvedValue('deadbeef');
		const code = await membershipService.createInvite('c1', 'readOnly');
		expect(code).toBe('deadbeef');
		expect(mockInvoke).toHaveBeenCalledWith('pro_create_invite', {
			collectionId: 'c1',
			permission: 'readOnly',
			email: undefined,
			ttlSecs: undefined
		});

		mockInvoke.mockClear();
		await membershipService.createInvite('c1', 'modify', 'a@b.com', 3600);
		expect(mockInvoke).toHaveBeenCalledWith('pro_create_invite', {
			collectionId: 'c1',
			permission: 'modify',
			email: 'a@b.com',
			ttlSecs: 3600
		});
	});

	it('listInvites maps expires_at → expiresAt', async () => {
		mockInvoke.mockResolvedValue([
			{ id: 'i1', code: 'abc', email: '', permission: 'modify', expires_at: '2026-07-09T00:00:00Z' }
		]);
		const invites = await membershipService.listInvites('c1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_list_invites', { collectionId: 'c1' });
		expect(invites).toEqual([
			{ id: 'i1', code: 'abc', email: '', permission: 'modify', expiresAt: '2026-07-09T00:00:00Z' }
		]);
	});

	it('listRequests maps requested_by / created_at', async () => {
		mockInvoke.mockResolvedValue([
			{ id: 'r1', requested_by: 'person-bob', created_at: '2026-07-02T00:00:00Z' }
		]);
		const reqs = await membershipService.listRequests('c1');
		expect(reqs).toEqual([
			{ id: 'r1', requestedBy: 'person-bob', createdAt: '2026-07-02T00:00:00Z' }
		]);
	});

	it('revokeInvite / approveRequest forward the right command + args', async () => {
		mockInvoke.mockResolvedValue(undefined);
		await membershipService.revokeInvite('i1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_revoke_invite', { inviteId: 'i1' });
		await membershipService.approveRequest('r1', 'modify');
		expect(mockInvoke).toHaveBeenCalledWith('pro_approve_request', {
			requestId: 'r1',
			permission: 'modify'
		});
	});

	it('currentPerson maps person_id → personId', async () => {
		mockInvoke.mockResolvedValue({ person_id: 'me', email: 'me@x.com' });
		const p = await membershipService.currentPerson();
		expect(mockInvoke).toHaveBeenCalledWith('pro_current_person');
		expect(p).toEqual({ personId: 'me', email: 'me@x.com' });
	});

	it('acceptInvite / requestJoin return the daemon detail', async () => {
		mockInvoke.mockResolvedValue('collection-x');
		expect(await membershipService.acceptInvite('code')).toBe('collection-x');
		expect(mockInvoke).toHaveBeenCalledWith('pro_accept_invite', { code: 'code' });
		mockInvoke.mockResolvedValue('req-1');
		expect(await membershipService.requestJoin('c1')).toBe('req-1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_request_join', { collectionId: 'c1' });
	});

	it('joinCollection forwards to pro_join_collection', async () => {
		mockInvoke.mockResolvedValue(undefined);
		await membershipService.joinCollection('c1');
		expect(mockInvoke).toHaveBeenCalledWith('pro_join_collection', { collectionId: 'c1' });
	});

	it('listJoinable maps rows to JoinableCollection[] (no args)', async () => {
		mockInvoke.mockResolvedValue([
			{ id: 'c-open', name: 'Marketing', restricted: false },
			{ id: 'c-r', name: 'Legal', restricted: true }
		]);
		const rows = await membershipService.listJoinable();
		expect(mockInvoke).toHaveBeenCalledWith('pro_list_joinable_collections');
		expect(rows).toEqual([
			{ id: 'c-open', name: 'Marketing', restricted: false },
			{ id: 'c-r', name: 'Legal', restricted: true }
		]);
	});
});
