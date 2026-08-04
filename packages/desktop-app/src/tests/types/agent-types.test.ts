import { describe, it, expect } from 'vitest';
import {
  isStreamingToken,
  isToolCallStart,
  isToolCallArgs,
  isStreamingDone,
  isStreamingError,
  isModelDownloading,
  isModelError,
  isAgentToolExecution,
  isAcpSessionFailed,
  AGENT_EVENTS,
  type StreamingChunk,
  type ModelStatus,
  type LocalAgentStatus,
  type AcpSessionState
} from '$lib/types/agent-types';

describe('streaming chunk type guards', () => {
  const token: StreamingChunk = { type: 'token', text: 'hi' };
  const toolCallStart: StreamingChunk = { type: 'tool_call_start', id: '1', name: 'search' };
  const toolCallArgs: StreamingChunk = { type: 'tool_call_args', id: '1', args_json: '{}' };
  const done: StreamingChunk = {
    type: 'done',
    usage: { prompt_tokens: 1, completion_tokens: 2 }
  };
  const error: StreamingChunk = { type: 'error', message: 'boom' };
  const cancelled: StreamingChunk = { type: 'cancelled' };

  it('isStreamingToken', () => {
    expect(isStreamingToken(token)).toBe(true);
    expect(isStreamingToken(done)).toBe(false);
  });

  it('isToolCallStart', () => {
    expect(isToolCallStart(toolCallStart)).toBe(true);
    expect(isToolCallStart(token)).toBe(false);
  });

  it('isToolCallArgs', () => {
    expect(isToolCallArgs(toolCallArgs)).toBe(true);
    expect(isToolCallArgs(token)).toBe(false);
  });

  it('isStreamingDone', () => {
    expect(isStreamingDone(done)).toBe(true);
    expect(isStreamingDone(error)).toBe(false);
  });

  it('isStreamingError', () => {
    expect(isStreamingError(error)).toBe(true);
    expect(isStreamingError(cancelled)).toBe(false);
  });
});

describe('model status type guards', () => {
  const downloading: ModelStatus = {
    status: 'downloading',
    progress_pct: 50,
    bytes_downloaded: 500,
    bytes_total: 1000
  };
  const errorStatus: ModelStatus = { status: 'error', message: 'failed' };
  const ready: ModelStatus = { status: 'ready' };

  it('isModelDownloading', () => {
    expect(isModelDownloading(downloading)).toBe(true);
    expect(isModelDownloading(ready)).toBe(false);
  });

  it('isModelError', () => {
    expect(isModelError(errorStatus)).toBe(true);
    expect(isModelError(ready)).toBe(false);
  });
});

describe('local agent status type guard', () => {
  const toolExecution: LocalAgentStatus = { status: 'tool_execution', tool_name: 'search_nodes' };
  const idle: LocalAgentStatus = { status: 'idle' };

  it('isAgentToolExecution', () => {
    expect(isAgentToolExecution(toolExecution)).toBe(true);
    expect(isAgentToolExecution(idle)).toBe(false);
  });
});

describe('ACP session state type guard', () => {
  const failed: AcpSessionState = { state: 'failed', reason: 'timeout' };
  const active: AcpSessionState = { state: 'active' };

  it('isAcpSessionFailed', () => {
    expect(isAcpSessionFailed(failed)).toBe(true);
    expect(isAcpSessionFailed(active)).toBe(false);
  });
});

describe('AGENT_EVENTS', () => {
  it('maps each key to its documented string value', () => {
    expect(AGENT_EVENTS.LOCAL_AGENT_CHUNK).toBe('local-agent://chunk');
    expect(AGENT_EVENTS.LOCAL_AGENT_TOOL).toBe('local-agent://tool');
    expect(AGENT_EVENTS.LOCAL_AGENT_STATUS).toBe('local-agent://status');
    expect(AGENT_EVENTS.LOCAL_AGENT_ERROR).toBe('local-agent://error');
    expect(AGENT_EVENTS.MODEL_DOWNLOAD_PROGRESS).toBe('model://download-progress');
    expect(AGENT_EVENTS.MODEL_DOWNLOAD_READY).toBe('model://download-ready');
    expect(AGENT_EVENTS.MODEL_STATUS).toBe('model://status');
    expect(AGENT_EVENTS.ACP_SESSION_STATE).toBe('acp://session-state');
    expect(AGENT_EVENTS.ACP_AGENT_MESSAGE).toBe('acp://agent-message');
  });
});
