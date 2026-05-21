/**
 * q2-preview diagnostics overlay (bd-b9kzg).
 *
 * Forked from `@quarto/preview-renderer`'s `PreviewErrorOverlay`
 * (per plan §Decision 3) so this surface can evolve with
 * q2-preview's needs without coupling to hub-client's. Extends
 * the source overlay's prop surface with:
 *
 *   - `severity` — drives header text + collapsed-indicator label
 *     so the warnings-only case ("3 warning(s)") doesn't have to
 *     borrow `error.message`.
 *   - `serverDiagnostics` — separate visual lane for diagnostics
 *     coming from the server-side sink (`/api/preview/diagnostics`),
 *     distinct from the WASM render's `error.diagnostics`.
 *
 * The fork keeps the upstream's CSS class names (`preview-error-*`)
 * so styling stays consistent; the new server-diagnostics group
 * gets its own `preview-error-server-diagnostics` class for the
 * fork-specific lane.
 */

import { useState } from 'react';
import type { Diagnostic, Pass1Failure } from '@quarto/preview-renderer/types/diagnostic';
import { stripAnsi } from '@quarto/preview-renderer/utils/stripAnsi';

import './PreviewDiagnosticsOverlay.css';

export type DiagnosticsOverlaySeverity = 'error' | 'warning';

export interface PreviewDiagnosticsOverlayProps {
  error: {
    message: string;
    diagnostics?: Diagnostic[];
    pass1Failures?: Pass1Failure[];
  } | null;
  visible: boolean;
  collapsed?: boolean;
  onToggleCollapsed?: (next: boolean) => void;
  /**
   * Severity hint. "warning" relabels the header / indicator so
   * the warnings-only case doesn't borrow the error message slot.
   * Defaults to "error" to preserve the source overlay's behaviour
   * when callers don't specify.
   */
  severity?: DiagnosticsOverlaySeverity;
  /**
   * Diagnostics from the server-side sink (capture_driver, deps,
   * re_execute). Rendered alongside `error.diagnostics` but in
   * their own visual group so the user can tell where the
   * diagnostic was raised. Empty array (or omitted) = no
   * server-side items.
   */
  serverDiagnostics?: Diagnostic[];
}

function severityLabel(severity: DiagnosticsOverlaySeverity): string {
  return severity === 'warning' ? 'Warning' : 'Error';
}

/**
 * Render a list of diagnostics, picking the rich ariadne snippet
 * when present and falling back to the compact one-line form
 * when it isn't (bd-352bh). Returns `null` for an empty list so
 * the caller can elide the surrounding wrapper.
 *
 * The "split into two visual modes" approach mirrors how
 * `q2 render` prints to stdout — every diagnostic with a
 * location gets the rich source-context box; diagnostics
 * without one (rare; project-level errors mostly) fall back to
 * the structured fields.
 */
function renderDiagnosticList(
  diagnostics: Diagnostic[],
  className: string,
): React.ReactElement | null {
  if (diagnostics.length === 0) return null;
  return (
    <div className={className}>
      {diagnostics.map((d, i) =>
        d.rendered ? (
          <pre key={i} className="preview-error-diagnostic-rendered">
            {stripAnsi(d.rendered)}
          </pre>
        ) : (
          <div key={i} className="preview-error-diagnostic-compact">
            {d.start_line != null && (
              <span className="diagnostic-line">Line {d.start_line}: </span>
            )}
            {d.code && <span className="diagnostic-code">[{d.code}] </span>}
            <span className="diagnostic-title">{d.title}</span>
            {d.problem && <span className="diagnostic-problem"> - {d.problem}</span>}
          </div>
        ),
      )}
    </div>
  );
}

export function PreviewDiagnosticsOverlay({
  error,
  visible,
  collapsed: controlledCollapsed,
  onToggleCollapsed,
  severity = 'error',
  serverDiagnostics = [],
}: PreviewDiagnosticsOverlayProps) {
  const [uncontrolledCollapsed, setUncontrolledCollapsed] = useState(true);
  const collapsed = controlledCollapsed ?? uncontrolledCollapsed;
  const setCollapsed = (next: boolean) => {
    if (onToggleCollapsed) onToggleCollapsed(next);
    if (controlledCollapsed === undefined) setUncontrolledCollapsed(next);
  };

  // Show the overlay if there's an error/warning payload OR
  // server diagnostics. The server-only case (empty error,
  // populated serverDiagnostics) is a valid render — Decision 1
  // wants those to surface too.
  const hasError = error !== null;
  const hasServerDiagnostics = serverDiagnostics.length > 0;
  if (!visible || (!hasError && !hasServerDiagnostics)) {
    return null;
  }

  const label = severityLabel(severity);
  const collapsedSeverityClass =
    severity === 'warning'
      ? 'preview-error-overlay--warning'
      : 'preview-error-overlay--error';

  if (collapsed) {
    // Collapsed state: minimal indicator
    return (
      <div
        className={`preview-error-overlay preview-error-overlay--collapsed ${collapsedSeverityClass}`}
      >
        <button
          className="preview-error-expand-btn"
          onClick={() => setCollapsed(false)}
          title={`Show ${label.toLowerCase()} details`}
        >
          <span className="preview-error-icon">&#9888;</span> {label}
        </button>
      </div>
    );
  }

  // Expanded state: full toast
  const cleanMessage = hasError ? stripAnsi(error.message) : '';
  return (
    <div
      className={`preview-error-overlay preview-error-overlay--expanded ${collapsedSeverityClass}`}
    >
      <div className="preview-error-header">
        <span className="preview-error-title">
          <span className="preview-error-icon">&#9888;</span> Render {label}
        </span>
        <button
          className="preview-error-collapse-btn"
          onClick={() => setCollapsed(true)}
          title="Collapse"
        >
          &minus;
        </button>
      </div>
      <div className="preview-error-content">
        {cleanMessage && <pre className="preview-error-message">{cleanMessage}</pre>}
        {hasError && error.diagnostics &&
          renderDiagnosticList(error.diagnostics, 'preview-error-diagnostics')}
        {hasError && error.pass1Failures && error.pass1Failures.length > 0 && (
          <div className="preview-error-pass1-failures">
            {error.pass1Failures.map((f, i) => (
              <div className="preview-error-pass1-failure" key={i}>
                <div className="diagnostic-source-file">
                  <span className="diagnostic-icon">&#9888;</span>{' '}
                  <code>{f.source_file}</code> failed to parse
                </div>
                {f.diagnostics.length > 0 ? (
                  renderDiagnosticList(f.diagnostics, 'preview-error-diagnostics')
                ) : (
                  <pre className="preview-error-message">{stripAnsi(f.error)}</pre>
                )}
              </div>
            ))}
          </div>
        )}
        {hasServerDiagnostics && (
          <div className="preview-error-server-diagnostics">
            <div className="preview-error-server-diagnostics-label">
              Server-side diagnostics
            </div>
            {renderDiagnosticList(serverDiagnostics, 'preview-error-diagnostics')}
          </div>
        )}
      </div>
    </div>
  );
}
