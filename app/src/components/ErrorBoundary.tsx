import { Component, type ReactNode } from "react";

import { ErrorState } from "@/components/states/StateViews";

interface Props {
  children: ReactNode;
}
interface State {
  error: Error | null;
}

/**
 * §45.2: no failure is silently swallowed. A render-time crash anywhere
 * in the shell surfaces a structured, honest error state instead of a
 * blank screen.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <ErrorState
          message={this.state.error.message}
          onRetry={() => this.setState({ error: null })}
        />
      );
    }
    return this.props.children;
  }
}
