import { Component, type ReactNode, type ErrorInfo } from 'react';
import { useDiagnostics } from '../diagnostics/store';

interface Props {
  children: ReactNode;
  /** Identifier shown in the diagnostics log and the error UI. */
  scope: string;
  /** Render override when crashed. Default = inline error card. */
  fallback?: (err: Error, scope: string, reset: () => void) => ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    useDiagnostics.getState().add({
      source: 'boundary',
      level: 'error',
      category: 'crash',
      field: this.props.scope,
      message: `[${this.props.scope}] ${error.message}`,
      detail: { stack: error.stack, componentStack: info.componentStack },
    });
  }

  reset = () => this.setState({ error: null });

  render() {
    if (this.state.error) {
      if (this.props.fallback) return this.props.fallback(this.state.error, this.props.scope, this.reset);
      return (
        <div className="error-boundary-card">
          <div className="error-boundary-header">
            <span className="error-boundary-dot" />
            <strong>{this.props.scope} crashed</strong>
            <button onClick={this.reset} className="error-boundary-reset">
              retry
            </button>
          </div>
          <div className="error-boundary-msg">{this.state.error.message}</div>
          <div className="error-boundary-hint">
            See diagnostics overlay (Shift+D) for full stack & wiring map.
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
