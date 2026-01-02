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
        <div className="bg-black h-screen flex flex-col items-center justify-center p-6">
          <div className="max-w-lg w-full">
            {/* Combined error box */}
            <div className="bg-neutral-900/50 border border-neutral-800/50 p-6 items-center text-center">
              <h1 className="text-6xl md:text-8xl text-red-500 mb-6 tracking-wider" style={{ fontFamily: "'Seven Segment', sans-serif" }}>
                ERROR
              </h1>
              
              <div className="space-y-3">
                <div>
                  <span className="text-[10px] text-neutral-600 tracking-[0.2em] uppercase block mb-1" style={{ fontFamily: "'Space Mono', monospace" }}>
                    Message
                  </span>
                  <p className="text-sm text-neutral-300" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {this.state.error?.message || "An unexpected error occurred"}
                  </p>
                </div>
                
                <div>
                  <span className="text-[10px] text-neutral-600 tracking-[0.2em] uppercase block mb-1" style={{ fontFamily: "'Space Mono', monospace" }}>
                    Route
                  </span>
                  <p className="text-sm text-neutral-300" style={{ fontFamily: "'Space Mono', monospace" }}>
                    {window.location.pathname}
                  </p>
                </div>
              </div>
              
              {/* Reset button */}
              <button
                onClick={this.handleReset}
                className="mt-6 w-full bg-neutral-800/70 border border-neutral-700 px-4 py-2 text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100 hover:border-red-500/50 transition-colors"
                style={{ fontFamily: "'Space Mono', monospace" }}
              >
                RELOAD
              </button>
              <button
                onClick={() => window.history.back()}
                className="mt-6 w-full bg-neutral-800/70 border border-neutral-700 px-4 py-2 text-sm text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100 hover:border-red-500/50 transition-colors"
                style={{ fontFamily: "'Space Mono', monospace" }}
              >
                BACK
              </button>
            </div>
          </div>
        </div>
      )
    }
    return this.props.children
  }
}
