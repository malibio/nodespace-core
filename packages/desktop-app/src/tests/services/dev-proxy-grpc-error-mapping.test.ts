/**
 * dev-proxy's grpcError() never attached
 * conflictData, so isSubtreeAccessDenied/isVersionConflict could never fire
 * against live browser/dev-server traffic — only hand-constructed HTTP test
 * bodies exercised the success path.
 *
 * This drives packages/dev-tools/src/grpc-error-mapping.ts's mapGrpcError()
 * DIRECTLY against fake gRPC-js ServiceErrors carrying the same trailer
 * metadata the daemon attaches (packages/daemon/src/services/node_service.rs,
 * ops_error_to_status) and asserts it derives the same code/conflictData
 * shape the Tauri command layer's status_to_command_error
 * (packages/desktop-app/src-tauri/src/commands/nodes.rs) derives from tonic
 * trailer metadata — the shape backend-adapter.test.ts already proves
 * HttpAdapter.handleResponse preserves once the JSON body carries it.
 */

import { describe, it, expect } from 'vitest';
import * as grpc from '@grpc/grpc-js';
import { mapGrpcError } from '../../../../dev-tools/src/grpc-error-mapping';

/** Build a fake gRPC-js ServiceError carrying the given code and trailers. */
function makeServiceError(
  code: grpc.status,
  details: string,
  trailers: Record<string, string> = {}
): grpc.ServiceError {
  const metadata = new grpc.Metadata();
  for (const [key, value] of Object.entries(trailers)) {
    metadata.set(key, value);
  }
  const err = new Error(details) as grpc.ServiceError;
  err.code = code;
  err.details = details;
  err.metadata = metadata;
  return err;
}

describe('dev-proxy grpc-error-mapping: mapGrpcError', () => {
  it('maps ABORTED + x-version-conflict metadata to VERSION_CONFLICT with the parsed payload', () => {
    const payload = { node_id: 'node-1', expected: 1, actual: 2, current_node: null };
    const err = makeServiceError(grpc.status.ABORTED, 'Version conflict on node-1: expected 1, got 2', {
      'x-version-conflict': JSON.stringify(payload)
    });

    const mapped = mapGrpcError(err);

    expect(mapped.code).toBe('VERSION_CONFLICT');
    expect(mapped.conflictData).toEqual(payload);
  });

  it('maps ABORTED without x-version-conflict metadata to VERSION_CONFLICT with no conflictData', () => {
    const err = makeServiceError(grpc.status.ABORTED, 'aborted for unrelated reasons');

    const mapped = mapGrpcError(err);

    expect(mapped.code).toBe('VERSION_CONFLICT');
    expect(mapped.conflictData).toBeUndefined();
  });

  it('maps FAILED_PRECONDITION + x-subtree-inaccessible-count metadata to SUBTREE_ACCESS_DENIED', () => {
    const err = makeServiceError(
      grpc.status.FAILED_PRECONDITION,
      'Delete refused: subtree contains 3 node(s) not accessible to the current actor',
      { 'x-subtree-inaccessible-count': '3' }
    );

    const mapped = mapGrpcError(err);

    expect(mapped.code).toBe('SUBTREE_ACCESS_DENIED');
    expect(mapped.conflictData).toEqual({ inaccessibleCount: 3 });
  });

  it('does NOT brand an unrelated FAILED_PRECONDITION (no count metadata) as a subtree refusal', () => {
    // FAILED_PRECONDITION also fires from node-create/schema validation paths
    // that carry no x-subtree-inaccessible-count metadata — status_to_command_error
    // gates on the metadata's presence, not the status code alone, and this
    // must match.
    const err = makeServiceError(grpc.status.FAILED_PRECONDITION, 'Node creation failed: bad schema');

    const mapped = mapGrpcError(err);

    expect(mapped.code).not.toBe('SUBTREE_ACCESS_DENIED');
    expect(mapped.conflictData).toBeUndefined();
  });

  it('falls back to the generic gRPC status name for unrelated codes, with no conflictData', () => {
    const err = makeServiceError(grpc.status.NOT_FOUND, 'Not found: node-1');

    const mapped = mapGrpcError(err);

    expect(mapped.code).toBe('NOT_FOUND');
    expect(mapped.conflictData).toBeUndefined();
  });

  it('ignores an unparseable x-subtree-inaccessible-count value rather than throwing', () => {
    const err = makeServiceError(grpc.status.FAILED_PRECONDITION, 'refused', {
      'x-subtree-inaccessible-count': 'not-a-number'
    });

    const mapped = mapGrpcError(err);

    expect(mapped.code).not.toBe('SUBTREE_ACCESS_DENIED');
    expect(mapped.conflictData).toBeUndefined();
  });

  it('ignores an unparseable x-version-conflict JSON value rather than throwing', () => {
    const err = makeServiceError(grpc.status.ABORTED, 'aborted', {
      'x-version-conflict': '{not valid json'
    });

    const mapped = mapGrpcError(err);

    expect(mapped.code).toBe('VERSION_CONFLICT');
    expect(mapped.conflictData).toBeUndefined();
  });

  it.each(['+3', '3abc', '-1', '3.5', ''])(
    'rejects a non-plain-digits x-subtree-inaccessible-count value (%s) like Rust str::parse::<u64>() would',
    (raw) => {
      const err = makeServiceError(grpc.status.FAILED_PRECONDITION, 'refused', {
        'x-subtree-inaccessible-count': raw
      });

      const mapped = mapGrpcError(err);

      expect(mapped.code).not.toBe('SUBTREE_ACCESS_DENIED');
      expect(mapped.conflictData).toBeUndefined();
    }
  );
});
