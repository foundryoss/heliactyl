import React from "react"

interface ErrorBoundaryProps {
  children: React.ReactNode
  fallback?: React.ReactNode
  onReset?: () => void
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends React.Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = { hasError: false, error: null }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error("ErrorBoundary caught an error:", error, errorInfo)
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null })
    this.props.onReset?.()
  }

  override render() {
    if (this.state.hasError) {
      return this.props.fallback ? (
        this.props.fallback
      ) : (
        <div className="bg-neutral-100 h-screen flex flex-col items-center justify-center min-h-[50vh] text-center p-6">
          <h1 className="text-2xl font-semibold text-black">
            Couldn't render this page.
          </h1>
          <p className="text-neutral-600 uppercase text-sm tracking-widest mt-2" style={{ fontFamily: 'Space Mono, sans-serif' }}>
            Error: {this.state.error?.message || "An unexpected error occurred."}
          </p>
        </div>
      )
    }
    return this.props.children
  }
}
