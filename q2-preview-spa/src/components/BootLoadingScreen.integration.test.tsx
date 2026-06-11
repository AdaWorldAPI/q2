/**
 * Tests for the boot loading screen (bd-jit6pdwq Phase 2).
 *
 * The screen has three jobs: show the initializing copy immediately,
 * surface retry progress (attempt counter + last error) so an endless
 * retry loop is never a silent spinner, and — after a threshold —
 * point Firefox users at the real culprit when the connection stalls
 * (another tab holding the per-IP WebSocket handshake queue).
 */

import { describe, it, expect } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { BootLoadingScreen } from './BootLoadingScreen';

describe('BootLoadingScreen', () => {
  it('shows initializing copy immediately, without the slow hint', () => {
    render(<BootLoadingScreen attempt={1} lastError={null} hintAfterMs={60_000} />);
    expect(screen.queryByText(/initializing q2-preview/i)).not.toBeNull();
    expect(screen.queryByText(/firefox/i)).toBeNull();
  });

  it('shows the Firefox hint after the threshold elapses', async () => {
    render(<BootLoadingScreen attempt={1} lastError={null} hintAfterMs={20} />);
    expect(screen.queryByText(/firefox/i)).toBeNull();
    await waitFor(() => {
      expect(screen.queryByText(/another tab/i)).not.toBeNull();
    });
    // The hint names the browser and the workaround.
    expect(screen.queryByText(/firefox/i)).not.toBeNull();
  });

  it('surfaces retry progress once attempts pass 1', () => {
    render(
      <BootLoadingScreen
        attempt={3}
        lastError={new Error('Document doc-1 is unavailable')}
        hintAfterMs={60_000}
      />,
    );
    // Not a silent spinner: the user can see it's retrying and why.
    expect(screen.queryByText(/retrying/i)).not.toBeNull();
    expect(screen.queryByText(/unavailable/i)).not.toBeNull();
  });

  it('shows no retry line on the first attempt', () => {
    render(<BootLoadingScreen attempt={1} lastError={null} hintAfterMs={60_000} />);
    expect(screen.queryByText(/retrying/i)).toBeNull();
  });
});
