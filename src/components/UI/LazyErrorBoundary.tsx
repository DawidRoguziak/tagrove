import { Component, type ErrorInfo, type ReactNode } from "react";

interface LazyErrorBoundaryProps {
  children: ReactNode;
  fallback: ReactNode;
  resetKey: string | number | boolean | null;
}

interface LazyErrorBoundaryState {
  failed: boolean;
}

export class LazyErrorBoundary extends Component<LazyErrorBoundaryProps, LazyErrorBoundaryState> {
  state: LazyErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): LazyErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Lazy UI chunk failed", error, info.componentStack);
  }

  componentDidUpdate(previousProps: LazyErrorBoundaryProps) {
    if (this.state.failed && previousProps.resetKey !== this.props.resetKey) {
      this.setState({ failed: false });
    }
  }

  render() {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}
