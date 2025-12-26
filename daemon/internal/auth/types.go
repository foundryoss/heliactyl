package auth

import (
	"context"
	"time"
)

// AuthManager handles authentication and authorization
type AuthManager interface {
	ValidateToken(ctx context.Context, token string) (*TokenInfo, error)
	CreateToken(ctx context.Context, req *CreateTokenRequest) (*Token, error)
	RevokeToken(ctx context.Context, tokenID string) error
	ListTokens(ctx context.Context) ([]*Token, error)
	LogSecurityEvent(ctx context.Context, event *SecurityEvent) error
}

// TokenInfo contains information about a validated token
type TokenInfo struct {
	ID          string            `json:"id"`
	Name        string            `json:"name"`
	Permissions []string          `json:"permissions"`
	ExpiresAt   *time.Time        `json:"expires_at,omitempty"`
	Metadata    map[string]string `json:"metadata,omitempty"`
	Valid       bool              `json:"valid"`
}

// Token represents an API token
type Token struct {
	ID          string            `json:"id"`
	Name        string            `json:"name"`
	Token       string            `json:"token,omitempty"` // Only returned on creation
	Permissions []string          `json:"permissions"`
	CreatedAt   time.Time         `json:"created_at"`
	ExpiresAt   *time.Time        `json:"expires_at,omitempty"`
	LastUsed    *time.Time        `json:"last_used,omitempty"`
	Metadata    map[string]string `json:"metadata,omitempty"`
	Active      bool              `json:"active"`
}

// CreateTokenRequest represents a request to create a new token
type CreateTokenRequest struct {
	Name        string            `json:"name"`
	Permissions []string          `json:"permissions"`
	ExpiresAt   *time.Time        `json:"expires_at,omitempty"`
	Metadata    map[string]string `json:"metadata,omitempty"`
}

// SecurityEvent represents a security-related event for audit logging
type SecurityEvent struct {
	ID         string            `json:"id"`
	Type       SecurityEventType `json:"type"`
	Timestamp  time.Time         `json:"timestamp"`
	TokenID    string            `json:"token_id,omitempty"`
	UserAgent  string            `json:"user_agent,omitempty"`
	RemoteAddr string            `json:"remote_addr,omitempty"`
	Resource   string            `json:"resource,omitempty"`
	Action     string            `json:"action,omitempty"`
	Result     EventResult       `json:"result"`
	Message    string            `json:"message"`
	Details    map[string]string `json:"details,omitempty"`
}

// SecurityEventType represents the type of security event
type SecurityEventType string

const (
	EventTypeAuthentication   SecurityEventType = "authentication"
	EventTypeAuthorization    SecurityEventType = "authorization"
	EventTypeTokenCreation    SecurityEventType = "token_creation"
	EventTypeTokenRevocation  SecurityEventType = "token_revocation"
	EventTypeAccessDenied     SecurityEventType = "access_denied"
	EventTypeInvalidToken     SecurityEventType = "invalid_token"
	EventTypePermissionDenied SecurityEventType = "permission_denied"
)

// EventResult represents the result of a security event
type EventResult string

const (
	ResultSuccess EventResult = "success"
	ResultFailure EventResult = "failure"
	ResultDenied  EventResult = "denied"
)

// Permission constants for role-based access control
const (
	PermissionContainerRead   = "container:read"
	PermissionContainerWrite  = "container:write"
	PermissionContainerDelete = "container:delete"
	PermissionFilesystemRead  = "filesystem:read"
	PermissionFilesystemWrite = "filesystem:write"
	PermissionSystemRead      = "system:read"
	PermissionSystemWrite     = "system:write"
	PermissionAdminRead       = "admin:read"
	PermissionAdminWrite      = "admin:write"
	PermissionAll             = "*"
)

// AuthError represents authentication/authorization errors
type AuthError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
	Details string `json:"details,omitempty"`
}

func (e *AuthError) Error() string {
	return e.Message
}

// Common auth error codes
const (
	ErrCodeInvalidToken      = "INVALID_TOKEN"
	ErrCodeExpiredToken      = "EXPIRED_TOKEN"
	ErrCodeInsufficientPerms = "INSUFFICIENT_PERMISSIONS"
	ErrCodeTokenNotFound     = "TOKEN_NOT_FOUND"
	ErrCodeTokenRevoked      = "TOKEN_REVOKED"
)
