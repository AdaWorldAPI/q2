/**
 * Boot controller for the q2-preview SPA (bd-jit6pdwq Phase 2).
 *
 * Design principle: **HTTP decides liveness; the WebSocket gets
 * patience.** Firefox serializes WebSocket opening handshakes per IP
 * address browser-wide (RFC 6455 §4.1, nsWSAdmissionManager), so a
 * hung handshake held by *another tab* can stall ours for tens of
 * seconds — no finite WS deadline picks the right answer. The preview
 * server's `/health` endpoint is plain HTTP, which that queue cannot
 * stall, so it is the only signal we trust for "the server is gone".
 *
 * While `/health` answers: retry `connect()` forever, with capped
 * exponential backoff. When `/health` fails `healthStrikes` times in a
 * row (post-failure confirmation, or the watchdog during a hung
 * attempt): throw {@link ServerGoneError} and let the UI show a
 * terminal "server stopped" message.
 *
 * Research: claude-notes/research/2026-06-11-firefox-ws-handshake-serialization.md
 * Plan: claude-notes/plans/2026-06-11-firefox-ws-peer-timeout-fix.md
 */

export class ServerGoneError extends Error {
  constructor() {
    super(
      'The preview server is not responding. ' +
        'It may have been stopped — restart `q2 preview` and reload this page.',
    );
    this.name = 'ServerGoneError';
  }
}

export interface BootStatus {
  phase: 'connecting';
  /** 1-based attempt counter. */
  attempt: number;
  /** The previous attempt's failure, if any. */
  lastError: Error | null;
}

export interface BootRetryOptions<T> {
  /** One connection attempt (the SPA's samod connect). */
  connect: () => Promise<T>;
  /**
   * Single `/health` probe; `true` means the server answered OK.
   * Must not throw — map fetch failures to `false`.
   */
  checkHealth: () => Promise<boolean>;
  /** Status updates for the UI, emitted before each attempt. */
  onStatus?: (status: BootStatus) => void;
  /** Cooperative cancellation (React effect unmount). */
  isCancelled?: () => boolean;
  /** First retry backoff. Default 1000 ms. */
  backoffBaseMs?: number;
  /** Backoff ceiling. Default 10000 ms. */
  backoffCapMs?: number;
  /** Watchdog poll spacing while an attempt is in flight. Default 2000 ms. */
  watchdogIntervalMs?: number;
  /** Spacing between post-failure confirmation probes. Default 500 ms. */
  confirmIntervalMs?: number;
  /** Consecutive failed probes that mean "server gone". Default 3. */
  healthStrikes?: number;
  /** Injectable for tests. */
  sleep?: (ms: number) => Promise<void>;
}

const defaultSleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

type WatchdogVerdict = 'gone' | 'cancelled';

/**
 * Connect with health-arbitrated retries.
 *
 * Resolves with `connect()`'s result, returns `null` if cancelled, or
 * throws {@link ServerGoneError} once `/health` confirms the server is
 * dead. Any other `connect()` error is retried (visible to the UI via
 * `onStatus`), never terminal — while the server is provably alive,
 * a stuck WebSocket or a cold-start sync race is always worth another
 * attempt, and a genuine bug keeps surfacing in the status line.
 */
export async function bootWithRetry<T>(options: BootRetryOptions<T>): Promise<T | null> {
  const {
    connect,
    checkHealth,
    onStatus,
    isCancelled = () => false,
    backoffBaseMs = 1000,
    backoffCapMs = 10000,
    watchdogIntervalMs = 2000,
    confirmIntervalMs = 500,
    healthStrikes = 3,
    sleep = defaultSleep,
  } = options;

  /**
   * Health watchdog raced against each in-flight attempt. Sleep-first:
   * a fast-settling attempt wins the race before the first probe, so
   * the happy path makes zero extra `/health` requests.
   */
  function runWatchdog(isStopped: () => boolean): Promise<WatchdogVerdict> {
    return (async () => {
      let strikes = 0;
      for (;;) {
        await sleep(watchdogIntervalMs);
        if (isStopped()) return 'cancelled';
        if (isCancelled()) return 'cancelled';
        const healthy = await checkHealth();
        if (isStopped()) return 'cancelled';
        if (isCancelled()) return 'cancelled';
        strikes = healthy ? 0 : strikes + 1;
        if (strikes >= healthStrikes) return 'gone';
      }
    })();
  }

  /**
   * Post-failure confirmation: is the server still there? Probes
   * back-to-back (spaced `confirmIntervalMs`) so one transient blip
   * doesn't read as death.
   */
  async function confirmHealth(): Promise<'healthy' | 'gone' | 'cancelled'> {
    for (let i = 0; i < healthStrikes; i++) {
      if (isCancelled()) return 'cancelled';
      if (await checkHealth()) return 'healthy';
      if (i < healthStrikes - 1) await sleep(confirmIntervalMs);
    }
    return 'gone';
  }

  let lastError: Error | null = null;
  for (let attempt = 1; ; attempt++) {
    if (isCancelled()) return null;
    onStatus?.({ phase: 'connecting', attempt, lastError });

    let raceSettled = false;
    // Start the attempt before the watchdog so a synchronously-settled
    // attempt is observed by the watchdog's first tick (otherwise an
    // immediate failure would still cost one health probe).
    const attemptPromise = connect();
    // The watchdog may win the race while connect is still pending; a
    // later rejection of the abandoned attempt must not become an
    // unhandled-rejection crash.
    attemptPromise.catch(() => {});
    let attemptDone = false;
    attemptPromise.then(
      () => { attemptDone = true; },
      () => { attemptDone = true; },
    );
    const watchdog = runWatchdog(() => raceSettled || attemptDone);

    try {
      const winner = await Promise.race([
        attemptPromise.then((files) => ({ kind: 'connected' as const, files })),
        watchdog.then((verdict) => ({ kind: verdict })),
      ]);
      raceSettled = true;

      if (winner.kind === 'connected') return winner.files;
      if (winner.kind === 'cancelled') return null;
      // Watchdog verdict 'gone': the server died while we were
      // patiently waiting on the WebSocket.
      throw new ServerGoneError();
    } catch (err) {
      raceSettled = true;
      if (err instanceof ServerGoneError) throw err;
      lastError = err instanceof Error ? err : new Error(String(err));
    }

    if (isCancelled()) return null;
    const verdict = await confirmHealth();
    if (verdict === 'cancelled') return null;
    if (verdict === 'gone') throw new ServerGoneError();

    // Healthy ⇒ the failure was transient (cold-start sync race, WS
    // queue contention). Back off and go again — no attempt cap.
    await sleep(Math.min(backoffBaseMs * 2 ** (attempt - 1), backoffCapMs));
  }
}
