import { Component, type ReactNode } from "react";
import { logDiagnostic } from "../tauri/diagnostics";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export default class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error("[ErrorBoundary]", error, info.componentStack);
    logDiagnostic("js_error", error.message, error.stack).catch(() => {});
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;
      return (
        <div className="error-boundary">
          <div className="error-boundary-panel">
            <div className="error-boundary-icon">💥</div>
            <h2 className="error-boundary-title">Something went wrong</h2>
            <p className="error-boundary-msg">
              {this.state.error?.message ?? "An unexpected error occurred."}
            </p>
            <p className="error-boundary-hint">
              Restart the application to continue. If this keeps happening,
              check the logs or contact support.
            </p>
            <button
              className="btn-primary"
              onClick={() => this.setState({ hasError: false, error: null })}
            >
              Try Again
            </button>
            <button
              className="btn-secondary"
              onClick={() => window.location.reload()}
              style={{ marginTop: 8 }}
            >
              Reload App
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
