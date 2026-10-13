// Keeps a bug in one screen from blanking the whole app: the screen shows a short apology
// with a way to try again, and the tab bar keeps working. Switching tabs resets it.

import { Component, type ReactNode } from 'react';

export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    console.error(error);
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main className="screen">
        <section className="pane empty-state">
          <strong>Something went wrong</strong>
          <p>This screen hit a problem. Your teams and saved data are safe.</p>
          <button className="go" onClick={() => this.setState({ failed: false })}>
            Try again
          </button>
        </section>
      </main>
    );
  }
}
