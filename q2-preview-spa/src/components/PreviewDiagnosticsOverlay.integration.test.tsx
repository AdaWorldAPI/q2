/**
 * Unit tests for PreviewDiagnosticsOverlay (bd-b9kzg).
 *
 * Pins the prop-surface extensions the q2-preview fork adds on
 * top of the upstream PreviewErrorOverlay (per plan §Decisions
 * and §Test plan item 6):
 *
 *   - `severity="warning"` switches the header / collapsed
 *     indicator from "Error" → "Warning" so warnings-only
 *     renders don't borrow the error message slot.
 *   - `serverDiagnostics` items render in their own visual
 *     group (distinct from `error.diagnostics`) so users can
 *     tell where each diagnostic originated.
 *   - Collapsed / expanded toggle behaves the same as the
 *     upstream overlay (regression — the fork must not break
 *     the existing UX).
 *   - The overlay can render with an empty `error` and only
 *     `serverDiagnostics` populated (server-only diagnostics
 *     should not require a synthetic `error` placeholder).
 */

import { describe, it, expect } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { Diagnostic } from '@quarto/preview-renderer/types/diagnostic';

import { PreviewDiagnosticsOverlay } from './PreviewDiagnosticsOverlay';

function diag(title: string, code?: string): Diagnostic {
  return {
    kind: 'warning',
    title,
    code,
    hints: [],
    details: [],
  };
}

describe('PreviewDiagnosticsOverlay', () => {
  it('renders nothing when not visible', () => {
    const { container } = render(
      <PreviewDiagnosticsOverlay error={null} visible={false} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows the warnings-mode collapsed indicator under severity="warning"', () => {
    render(
      <PreviewDiagnosticsOverlay
        error={{
          message: '',
          diagnostics: [diag('Body link references missing document', 'Q-13-4')],
        }}
        visible
        collapsed
        severity="warning"
      />,
    );
    // Warnings-mode collapsed label should read "warning", not
    // "error" (the upstream component's wording).
    expect(screen.queryByText(/warning/i)).not.toBeNull();
    expect(screen.queryByText(/^error$/i)).toBeNull();
  });

  it('shows the error-mode collapsed indicator under severity="error" (default)', () => {
    render(
      <PreviewDiagnosticsOverlay
        error={{
          message: 'Render failure',
          diagnostics: [diag('Synthetic error')],
        }}
        visible
        collapsed
        severity="error"
      />,
    );
    expect(screen.queryByText(/error/i)).not.toBeNull();
  });

  it('expands and collapses on user click (controlled mode)', () => {
    let collapsed = true;
    const setCollapsed = (next: boolean) => {
      collapsed = next;
    };

    const { rerender } = render(
      <PreviewDiagnosticsOverlay
        error={{ message: 'msg', diagnostics: [diag('A diagnostic')] }}
        visible
        collapsed={collapsed}
        onToggleCollapsed={setCollapsed}
        severity="warning"
      />,
    );

    // Click the collapsed-mode expand affordance. The fork uses
    // the same button-driven expand as the upstream overlay.
    const expandBtn = screen.getByRole('button');
    fireEvent.click(expandBtn);
    expect(collapsed).toBe(false);

    // Re-render with the updated collapsed state.
    rerender(
      <PreviewDiagnosticsOverlay
        error={{ message: 'msg', diagnostics: [diag('A diagnostic')] }}
        visible
        collapsed={collapsed}
        onToggleCollapsed={setCollapsed}
        severity="warning"
      />,
    );

    // The expanded view should now show the diagnostic title.
    expect(screen.queryByText(/A diagnostic/)).not.toBeNull();
  });

  it('renders server diagnostics in their own visual group', () => {
    render(
      <PreviewDiagnosticsOverlay
        error={{
          message: '',
          diagnostics: [diag('WASM diagnostic')],
        }}
        visible
        collapsed={false}
        severity="warning"
        serverDiagnostics={[diag('Server diagnostic', 'Q-CAP-1')]}
      />,
    );

    // Both diagnostics surface. The visual grouping is asserted
    // by looking for a server-section marker the fork emits
    // (class or label) — the exact selector lives in the impl.
    expect(screen.queryByText(/WASM diagnostic/)).not.toBeNull();
    expect(screen.queryByText(/Server diagnostic/)).not.toBeNull();
    // Pin the lane separation: the server group's wrapper has
    // a stable class the fork advertises.
    const serverGroup = document.querySelector(
      '.preview-error-server-diagnostics',
    );
    expect(serverGroup).not.toBeNull();
  });

  it('renders with empty error + only serverDiagnostics populated', () => {
    // The server-only case: WASM render succeeded with no
    // warnings but the server-side sink picked up an
    // eager-capture failure for a sibling. The overlay must
    // render without requiring a synthetic error placeholder.
    render(
      <PreviewDiagnosticsOverlay
        error={null}
        visible
        collapsed={false}
        severity="warning"
        serverDiagnostics={[diag('Eager capture failed', 'Q-CAP-1')]}
      />,
    );
    expect(screen.queryByText(/Eager capture failed/)).not.toBeNull();
  });
});
