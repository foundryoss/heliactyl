package auth

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"strings"

	"NightLightd/internal/config"

	"github.com/gorilla/mux"
)

// Middleware provides HTTP authentication middleware
type Middleware struct {
	authManager AuthManager
	config      *config.Config // For legacy BasicAuth support (will be deprecated soon.)
}

// NewMiddleware creates a new authentication middleware (legacy - accepts *config.Config)
func NewMiddleware(cfg *config.Config) *Middleware {
	return &Middleware{
		config: cfg,
	}
}

// NewMiddlewareWithManager creates a new authentication middleware with AuthManager
func NewMiddlewareWithManager(authManager AuthManager) *Middleware {
	return &Middleware{
		authManager: authManager,
	}
}

// contextKey is used for storing values in request context
type contextKey string

const (
	TokenInfoKey contextKey = "token_info"
)

// BasicAuth provides basic authentication middleware (legacy)
func (m *Middleware) BasicAuth(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		auth := r.Header.Get("Authorization")
		if auth == "" {
			m.unauthorized(w)
			return
		}

		if !strings.HasPrefix(auth, "Basic ") {
			m.unauthorized(w)
			return
		}

		encoded := strings.TrimPrefix(auth, "Basic ")
		decoded, err := base64.StdEncoding.DecodeString(encoded)
		if err != nil {
			m.unauthorized(w)
			return
		}

		credentials := strings.SplitN(string(decoded), ":", 2)
		if len(credentials) != 2 {
			m.unauthorized(w)
			return
		}

		username, password := credentials[0], credentials[1]

		if username != "NightLight" || password != m.config.Key {
			m.unauthorized(w)
			return
		}

		next.ServeHTTP(w, r)
	})
}

// unauthorized sends an unauthorized response
func (m *Middleware) unauthorized(w http.ResponseWriter) {
	w.Header().Set("WWW-Authenticate", `Basic realm="NightLight Daemon"`)
	w.WriteHeader(http.StatusUnauthorized)
	w.Write([]byte("Unauthorized"))
}

// RequireAuth is middleware that requires valid authentication
func (m *Middleware) RequireAuth(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		tokenInfo, err := m.authenticateRequest(r)
		if err != nil {
			m.writeAuthError(w, r, err)
			return
		}

		ctx := context.WithValue(r.Context(), TokenInfoKey, tokenInfo)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// RequirePermission is middleware that requires specific permissions
func (m *Middleware) RequirePermission(permission string) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			tokenInfo, err := m.authenticateRequest(r)
			if err != nil {
				m.writeAuthError(w, r, err)
				return
			}

			if manager, ok := m.authManager.(*Manager); ok {
				if !manager.HasPermission(tokenInfo, permission) {
					m.logPermissionDenied(r, tokenInfo, permission)
					m.writePermissionError(w, r, permission)
					return
				}
			}

			ctx := context.WithValue(r.Context(), TokenInfoKey, tokenInfo)
			next.ServeHTTP(w, r.WithContext(ctx))
		})
	}
}

// RequireContainerAccess is middleware that requires specific container permissions
func (m *Middleware) RequireContainerAccess(operation string) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			tokenInfo, err := m.authenticateRequest(r)
			if err != nil {
				m.writeAuthError(w, r, err)
				return
			}

			containerID := m.extractContainerID(r)
			if containerID == "" {
				m.writeError(w, http.StatusBadRequest, "MISSING_CONTAINER_ID", "Container ID is required")
				return
			}

			ctx := context.WithValue(r.Context(), TokenInfoKey, tokenInfo)
			next.ServeHTTP(w, r.WithContext(ctx))
		})
	}
}

// GetTokenInfo extracts token info from request context
func GetTokenInfo(r *http.Request) *TokenInfo {
	if tokenInfo, ok := r.Context().Value(TokenInfoKey).(*TokenInfo); ok {
		return tokenInfo
	}
	return nil
}

// authenticateRequest extracts and validates the authentication token from the request
func (m *Middleware) authenticateRequest(r *http.Request) (*TokenInfo, error) {
	token := m.extractToken(r)
	if token == "" {
		return nil, &AuthError{
			Code:    ErrCodeInvalidToken,
			Message: "Authentication token required",
		}
	}

	if m.authManager == nil {
		return nil, &AuthError{
			Code:    ErrCodeInvalidToken,
			Message: "Auth manager not configured",
		}
	}

	return m.authManager.ValidateToken(r.Context(), token)
}

// extractToken extracts the authentication token from the request
func (m *Middleware) extractToken(r *http.Request) string {
	authHeader := r.Header.Get("Authorization")
	if authHeader != "" {
		parts := strings.SplitN(authHeader, " ", 2)
		if len(parts) == 2 && strings.ToLower(parts[0]) == "bearer" {
			return parts[1]
		}
	}

	token := r.URL.Query().Get("token")
	if token != "" {
		return token
	}

	token = r.Header.Get("X-API-Token")
	if token != "" {
		return token
	}

	return ""
}

// writeAuthError writes an authentication error response
func (m *Middleware) writeAuthError(w http.ResponseWriter, r *http.Request, err error) {
	if m.authManager != nil {
		m.authManager.LogSecurityEvent(r.Context(), &SecurityEvent{
			Type:       EventTypeAuthentication,
			Result:     ResultFailure,
			Message:    err.Error(),
			Resource:   r.URL.Path,
			Action:     r.Method,
			UserAgent:  r.Header.Get("User-Agent"),
			RemoteAddr: r.RemoteAddr,
		})
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusUnauthorized)

	response := map[string]any{
		"success": false,
		"error": map[string]any{
			"code":    "AUTHENTICATION_FAILED",
			"message": "Authentication required",
		},
	}

	json.NewEncoder(w).Encode(response)
}

// writePermissionError writes a permission denied error response
func (m *Middleware) writePermissionError(w http.ResponseWriter, r *http.Request, permission string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusForbidden)

	response := map[string]any{
		"success": false,
		"error": map[string]any{
			"code":    "INSUFFICIENT_PERMISSIONS",
			"message": "Insufficient permissions for this operation",
			"details": map[string]any{
				"required_permission": permission,
				"resource":            r.URL.Path,
				"action":              r.Method,
			},
		},
	}

	json.NewEncoder(w).Encode(response)
}

// logPermissionDenied logs a permission denied event
func (m *Middleware) logPermissionDenied(r *http.Request, tokenInfo *TokenInfo, permission string) {
	if m.authManager != nil {
		m.authManager.LogSecurityEvent(r.Context(), &SecurityEvent{
			Type:       EventTypePermissionDenied,
			TokenID:    tokenInfo.ID,
			Result:     ResultDenied,
			Message:    "Permission denied",
			Resource:   r.URL.Path,
			Action:     r.Method,
			UserAgent:  r.Header.Get("User-Agent"),
			RemoteAddr: r.RemoteAddr,
			Details: map[string]string{
				"required_permission": permission,
				"user_permissions":    strings.Join(tokenInfo.Permissions, ","),
			},
		})
	}
}

// extractContainerID extracts container ID from URL path
func (m *Middleware) extractContainerID(r *http.Request) string {
	vars := mux.Vars(r)
	if containerID, exists := vars["containerId"]; exists {
		return containerID
	}
	if containerID, exists := vars["containerID"]; exists {
		return containerID
	}
	return ""
}

// writeError writes a generic error response
func (m *Middleware) writeError(w http.ResponseWriter, statusCode int, code, message string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(statusCode)

	response := map[string]any{
		"success": false,
		"error": map[string]any{
			"code":    code,
			"message": message,
		},
	}

	json.NewEncoder(w).Encode(response)
}

// OptionalAuth is middleware that extracts auth info if present but doesn't require it
func (m *Middleware) OptionalAuth(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		tokenInfo, _ := m.authenticateRequest(r)
		ctx := context.WithValue(r.Context(), TokenInfoKey, tokenInfo)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}
