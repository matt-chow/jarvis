import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportError } from "../hooks/reportError";
import { Button, Panel } from "./ui";

// Provisional component: awaiting design integration (see docs/MODULES.md › Design status).

interface Props {
  /** Which module this wraps (for the report). */
  module: string;
  children: ReactNode;
}

/**
 * Keeps a crash in one module from taking down the app (DECISIONS D-027): the
 * shell and navigation stay usable, the error is reported, and the module can
 * be retried. Data is server-side or in IndexedDB, so nothing is lost.
 */
export class ErrorBoundary extends Component<Props, { error?: Error }> {
  state: { error?: Error } = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    reportError(Object.assign(error, { stack: `${error.stack ?? ""}\n${info.componentStack ?? ""}` }), this.props.module);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <Panel title="something went wrong">
        <div role="alert">
          <p className="text-sm">This screen hit a problem and stopped. Your data is safe, and the rest of the app still works.</p>
          <p className="mt-2 font-mono text-[11px] break-words text-muted">{this.state.error.message}</p>
        </div>
        <div className="mt-3 flex gap-2">
          <Button variant="primary" onClick={() => this.setState({ error: undefined })}>
            try again
          </Button>
          <Button onClick={() => location.reload()}>reload app</Button>
        </div>
        <p className="mt-3 text-xs text-muted">The error was recorded in Settings › diagnostics.</p>
      </Panel>
    );
  }
}
