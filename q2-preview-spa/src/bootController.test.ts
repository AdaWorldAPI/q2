/**
 * Unit tests for the boot controller (bd-jit6pdwq Phase 2).
 *
 * These are the durable, browser-free regression guards for the
 * Firefox WS-handshake-serialization fix: the SPA must keep trying the
 * WebSocket while HTTP `/health` says the server is alive (a finite WS
 * deadline can't be made correct under Firefox's per-IP handshake
 * queue), and must declare the server gone only on `/health` evidence.
 *
 * Plan: claude-notes/plans/2026-06-11-firefox-ws-peer-timeout-fix.md
 */

import { describe, it, expect, vi } from 'vitest';
import { bootWithRetry, ServerGoneError } from './bootController';
import type { FileEntry } from '@quarto/quarto-automerge-schema';

const FILES: FileEntry[] = [{ path: 'index.qmd', docId: 'doc-1' }];

/** Instant sleep that records requested durations. */
function recordingSleep() {
  const calls: number[] = [];
  const sleep = (ms: number) => {
    calls.push(ms);
    return Promise.resolve();
  };
  return { calls, sleep };
}

/** Tuning that makes every delay instant for tests. */
function instantTuning(overrides: Record<string, unknown> = {}) {
  const { calls, sleep } = recordingSleep();
  return {
    tuning: {
      backoffBaseMs: 1000,
      backoffCapMs: 10000,
      watchdogIntervalMs: 2000,
      confirmIntervalMs: 500,
      healthStrikes: 3,
      sleep,
      ...overrides,
    },
    sleepCalls: calls,
  };
}

describe('bootWithRetry', () => {
  it('returns files when connect succeeds first try', async () => {
    const connect = vi.fn().mockResolvedValue(FILES);
    const checkHealth = vi.fn().mockResolvedValue(true);
    const { tuning } = instantTuning();

    const result = await bootWithRetry({ connect, checkHealth, ...tuning });

    expect(result).toEqual(FILES);
    expect(connect).toHaveBeenCalledTimes(1);
  });

  it('retries after a connect failure while /health is OK', async () => {
    const connect = vi
      .fn()
      .mockRejectedValueOnce(new Error('Document x is unavailable'))
      .mockResolvedValueOnce(FILES);
    const checkHealth = vi.fn().mockResolvedValue(true);
    const statuses: unknown[] = [];
    const { tuning } = instantTuning();

    const result = await bootWithRetry({
      connect,
      checkHealth,
      onStatus: (s) => statuses.push(s),
      ...tuning,
    });

    expect(result).toEqual(FILES);
    expect(connect).toHaveBeenCalledTimes(2);
    // The retry was visible to the UI, carrying the failure.
    expect(statuses).toContainEqual(
      expect.objectContaining({
        phase: 'connecting',
        attempt: 2,
        lastError: expect.objectContaining({ message: expect.stringMatching(/unavailable/) }),
      }),
    );
  });

  it('keeps retrying without an attempt cap while healthy', async () => {
    let attempts = 0;
    const connect = vi.fn().mockImplementation(() => {
      attempts++;
      return attempts < 8
        ? Promise.reject(new Error(`fail ${attempts}`))
        : Promise.resolve(FILES);
    });
    const checkHealth = vi.fn().mockResolvedValue(true);
    const { tuning } = instantTuning();

    const result = await bootWithRetry({ connect, checkHealth, ...tuning });

    expect(result).toEqual(FILES);
    expect(connect).toHaveBeenCalledTimes(8);
  });

  it('applies exponential backoff between attempts, capped', async () => {
    let attempts = 0;
    const connect = vi.fn().mockImplementation(() => {
      attempts++;
      return attempts < 7
        ? Promise.reject(new Error('nope'))
        : Promise.resolve(FILES);
    });
    const checkHealth = vi.fn().mockResolvedValue(true);
    // Sentinel watchdog interval: the watchdog's per-attempt sleep is
    // unavoidable (instant in this fixture), so give it a duration the
    // filter below can exclude exactly.
    const { tuning, sleepCalls } = instantTuning({ watchdogIntervalMs: 7777 });

    await bootWithRetry({ connect, checkHealth, ...tuning });

    // Exclude watchdog ticks (7777) and confirm spacing (500): what
    // remains are the backoffs. 1s, 2s, 4s, 8s → capped at 10s.
    const backoffs = sleepCalls.filter((ms) => ms !== 7777 && ms !== 500);
    expect(backoffs).toEqual([1000, 2000, 4000, 8000, 10000, 10000]);
  });

  it('throws ServerGoneError when /health fails after a connect failure', async () => {
    const connect = vi.fn().mockRejectedValue(new Error('boom'));
    const checkHealth = vi.fn().mockResolvedValue(false);
    const { tuning } = instantTuning();

    await expect(bootWithRetry({ connect, checkHealth, ...tuning })).rejects.toBeInstanceOf(
      ServerGoneError,
    );
    // healthStrikes consecutive probes confirm death — no premature
    // verdict on one blip.
    expect(checkHealth).toHaveBeenCalledTimes(3);
    expect(connect).toHaveBeenCalledTimes(1);
  });

  it('tolerates a transient /health blip (one false, then true ⇒ retry, not terminal)', async () => {
    const connect = vi
      .fn()
      .mockRejectedValueOnce(new Error('boom'))
      .mockResolvedValueOnce(FILES);
    const checkHealth = vi
      .fn()
      .mockResolvedValueOnce(false)
      .mockResolvedValue(true);
    const { tuning } = instantTuning();

    const result = await bootWithRetry({ connect, checkHealth, ...tuning });

    expect(result).toEqual(FILES);
    expect(connect).toHaveBeenCalledTimes(2);
  });

  it('declares the server gone while connect hangs (watchdog)', async () => {
    // connect never settles — the indefinite-peer-wait case where the
    // user kills the server mid-wait.
    const connect = vi.fn().mockImplementation(() => new Promise(() => {}));
    const checkHealth = vi.fn().mockResolvedValue(false);
    const { tuning } = instantTuning();

    await expect(bootWithRetry({ connect, checkHealth, ...tuning })).rejects.toBeInstanceOf(
      ServerGoneError,
    );
    expect(checkHealth).toHaveBeenCalledTimes(3);
  });

  it('does not fire the watchdog while /health stays healthy during a hang, and resolves when connect finally does', async () => {
    let resolveConnect!: (files: FileEntry[]) => void;
    const connect = vi.fn().mockImplementation(
      () => new Promise<FileEntry[]>((res) => { resolveConnect = res; }),
    );
    let healthPolls = 0;
    const checkHealth = vi.fn().mockImplementation(() => {
      healthPolls++;
      // Resolve the hung connect after a few healthy polls — the
      // Firefox slot freeing up.
      if (healthPolls === 5) resolveConnect(FILES);
      return Promise.resolve(true);
    });
    const { tuning } = instantTuning();

    const result = await bootWithRetry({ connect, checkHealth, ...tuning });

    expect(result).toEqual(FILES);
    expect(connect).toHaveBeenCalledTimes(1);
    expect(healthPolls).toBeGreaterThanOrEqual(5);
  });

  it('returns null when cancelled between attempts', async () => {
    let cancelled = false;
    const connect = vi.fn().mockImplementation(() => {
      cancelled = true; // simulate unmount while the attempt is in flight
      return Promise.reject(new Error('boom'));
    });
    const checkHealth = vi.fn().mockResolvedValue(true);
    const { tuning } = instantTuning();

    const result = await bootWithRetry({
      connect,
      checkHealth,
      isCancelled: () => cancelled,
      ...tuning,
    });

    expect(result).toBeNull();
    expect(connect).toHaveBeenCalledTimes(1);
  });

  it('stops watchdog polling after cancellation during a hang', async () => {
    let cancelled = false;
    const connect = vi.fn().mockImplementation(() => new Promise(() => {}));
    let polls = 0;
    const checkHealth = vi.fn().mockImplementation(() => {
      polls++;
      if (polls === 3) cancelled = true;
      return Promise.resolve(true);
    });
    const { tuning } = instantTuning();

    const result = await bootWithRetry({
      connect,
      checkHealth,
      isCancelled: () => cancelled,
      ...tuning,
    });

    expect(result).toBeNull();
    // The watchdog noticed cancellation and stopped; a runaway loop
    // would have kept polling far past 3-4.
    expect(polls).toBeLessThanOrEqual(4);
  });
});
