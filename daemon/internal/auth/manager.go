package auth

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/sirupsen/logrus"
)

// Manager implements the AuthManager interface
type Manager struct {
	logger      *logrus.Logger
	tokens      map[string]*Token // token hash -> token info
	tokensByID  map[string]*Token // token ID -> token info
	events      []*SecurityEvent
	mu          sync.RWMutex
	eventsMu    sync.RWMutex
	maxEvents   int
	configToken string // Token from configuration
}

// NewManager creates a new authentication manager
func NewManager(logger *logrus.Logger, configToken string) *Manager {
	manager := &Manager{
		logger:      logger,
		tokens:      make(map[string]*Token),
		tokensByID:  make(map[string]*Token),
		events:      make([]*SecurityEvent, 0),
		maxEvents:   10000,
		configToken: configToken,
	}

	// Add the configuration token as a master token
	if configToken != "" {
		masterToken := &Token{
			ID:          "master",
			Name:        "Master Token",
			Permissions: []string{PermissionAll},
			CreatedAt:   time.Now(),
			Active:      true,
			Metadata: map[string]string{
				"source": "configuration",
				"type":   "master",
			},
		}

		tokenHash := manager.hashToken(configToken)
		manager.tokens[tokenHash] = masterToken
		manager.tokensByID["master"] = masterToken

		logger.Info("Master token configured from configuration file")
	}

	return manager
}

// ValidateToken validates an API token and returns token information
func (m *Manager) ValidateToken(ctx context.Context, token string) (*TokenInfo, error) {
	if token == "" {
		m.logSecurityEvent(ctx, &SecurityEvent{
			Type:    EventTypeAuthentication,
			Result:  ResultFailure,
			Message: "Empty token provided",
		})
		return nil, &AuthError{
			Code:    ErrCodeInvalidToken,
			Message: "Token is required",
		}
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	tokenHash := m.hashToken(token)
	tokenInfo, exists := m.tokens[tokenHash]
	if !exists {
		m.logSecurityEvent(ctx, &SecurityEvent{
			Type:    EventTypeAuthentication,
			Result:  ResultFailure,
			Message: "Invalid token provided",
		})
		return nil, &AuthError{
			Code:    ErrCodeInvalidToken,
			Message: "Invalid token",
		}
	}

	if !tokenInfo.Active {
		m.logSecurityEvent(ctx, &SecurityEvent{
			Type:    EventTypeAuthentication,
			TokenID: tokenInfo.ID,
			Result:  ResultFailure,
			Message: "Revoked token used",
		})
		return nil, &AuthError{
			Code:    ErrCodeTokenRevoked,
			Message: "Token has been revoked",
		}
	}

	if tokenInfo.ExpiresAt != nil && time.Now().After(*tokenInfo.ExpiresAt) {
		m.logSecurityEvent(ctx, &SecurityEvent{
			Type:    EventTypeAuthentication,
			TokenID: tokenInfo.ID,
			Result:  ResultFailure,
			Message: "Expired token used",
		})
		return nil, &AuthError{
			Code:    ErrCodeExpiredToken,
			Message: "Token has expired",
		}
	}

	now := time.Now()
	tokenInfo.LastUsed = &now

	m.logSecurityEvent(ctx, &SecurityEvent{
		Type:    EventTypeAuthentication,
		TokenID: tokenInfo.ID,
		Result:  ResultSuccess,
		Message: "Token validated successfully",
	})

	return &TokenInfo{
		ID:          tokenInfo.ID,
		Name:        tokenInfo.Name,
		Permissions: tokenInfo.Permissions,
		ExpiresAt:   tokenInfo.ExpiresAt,
		Metadata:    tokenInfo.Metadata,
		Valid:       true,
	}, nil
}

// CreateToken creates a new API token
func (m *Manager) CreateToken(ctx context.Context, req *CreateTokenRequest) (*Token, error) {
	if req.Name == "" {
		return nil, &AuthError{
			Code:    "INVALID_REQUEST",
			Message: "Token name is required",
		}
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	tokenID := m.generateID()

	tokenBytes := make([]byte, 32)
	if _, err := rand.Read(tokenBytes); err != nil {
		return nil, fmt.Errorf("failed to generate token: %w", err)
	}
	tokenString := hex.EncodeToString(tokenBytes)

	token := &Token{
		ID:          tokenID,
		Name:        req.Name,
		Token:       tokenString,
		Permissions: req.Permissions,
		CreatedAt:   time.Now(),
		ExpiresAt:   req.ExpiresAt,
		Metadata:    req.Metadata,
		Active:      true,
	}

	tokenHash := m.hashToken(tokenString)
	m.tokens[tokenHash] = token
	m.tokensByID[tokenID] = token

	m.logSecurityEvent(ctx, &SecurityEvent{
		Type:    EventTypeTokenCreation,
		TokenID: tokenID,
		Result:  ResultSuccess,
		Message: fmt.Sprintf("Token '%s' created", req.Name),
		Details: map[string]string{
			"permissions": strings.Join(req.Permissions, ","),
		},
	})

	m.logger.WithFields(logrus.Fields{
		"token_id":    tokenID,
		"token_name":  req.Name,
		"permissions": req.Permissions,
	}).Info("API token created")

	return token, nil
}

// RevokeToken revokes an API token
func (m *Manager) RevokeToken(ctx context.Context, tokenID string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	token, exists := m.tokensByID[tokenID]
	if !exists {
		return &AuthError{
			Code:    ErrCodeTokenNotFound,
			Message: "Token not found",
		}
	}

	if tokenID == "master" {
		return &AuthError{
			Code:    "CANNOT_REVOKE_MASTER",
			Message: "Cannot revoke master token",
		}
	}

	token.Active = false

	m.logSecurityEvent(ctx, &SecurityEvent{
		Type:    EventTypeTokenRevocation,
		TokenID: tokenID,
		Result:  ResultSuccess,
		Message: fmt.Sprintf("Token '%s' revoked", token.Name),
	})

	m.logger.WithFields(logrus.Fields{
		"token_id":   tokenID,
		"token_name": token.Name,
	}).Info("API token revoked")

	return nil
}

// ListTokens returns all tokens (without the actual token values)
func (m *Manager) ListTokens(ctx context.Context) ([]*Token, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	tokens := make([]*Token, 0, len(m.tokensByID))
	for _, token := range m.tokensByID {
		tokenCopy := *token
		tokenCopy.Token = ""
		tokens = append(tokens, &tokenCopy)
	}

	return tokens, nil
}

// LogSecurityEvent logs a security event
func (m *Manager) LogSecurityEvent(ctx context.Context, event *SecurityEvent) error {
	return m.logSecurityEvent(ctx, event)
}

// HasPermission checks if a token has a specific permission
func (m *Manager) HasPermission(tokenInfo *TokenInfo, permission string) bool {
	if tokenInfo == nil {
		return false
	}

	for _, perm := range tokenInfo.Permissions {
		if perm == PermissionAll {
			return true
		}
		if perm == permission {
			return true
		}
	}

	return false
}

// GetSecurityEvents returns recent security events
func (m *Manager) GetSecurityEvents(ctx context.Context, limit int) ([]*SecurityEvent, error) {
	m.eventsMu.RLock()
	defer m.eventsMu.RUnlock()

	if limit <= 0 || limit > len(m.events) {
		limit = len(m.events)
	}

	start := len(m.events) - limit
	if start < 0 {
		start = 0
	}

	events := make([]*SecurityEvent, limit)
	copy(events, m.events[start:])

	return events, nil
}

func (m *Manager) logSecurityEvent(ctx context.Context, event *SecurityEvent) error {
	m.eventsMu.Lock()
	defer m.eventsMu.Unlock()

	if event.ID == "" {
		event.ID = m.generateID()
	}
	if event.Timestamp.IsZero() {
		event.Timestamp = time.Now()
	}

	m.events = append(m.events, event)

	if len(m.events) > m.maxEvents {
		copy(m.events, m.events[len(m.events)-m.maxEvents:])
		m.events = m.events[:m.maxEvents]
	}

	logFields := logrus.Fields{
		"event_id":   event.ID,
		"event_type": event.Type,
		"result":     event.Result,
		"message":    event.Message,
	}

	if event.TokenID != "" {
		logFields["token_id"] = event.TokenID
	}
	if event.Resource != "" {
		logFields["resource"] = event.Resource
	}
	if event.Action != "" {
		logFields["action"] = event.Action
	}

	switch event.Result {
	case ResultSuccess:
		m.logger.WithFields(logFields).Info("Security event")
	case ResultFailure, ResultDenied:
		m.logger.WithFields(logFields).Warn("Security event")
	default:
		m.logger.WithFields(logFields).Info("Security event")
	}

	return nil
}

func (m *Manager) hashToken(token string) string {
	hash := sha256.Sum256([]byte(token))
	return hex.EncodeToString(hash[:])
}

func (m *Manager) generateID() string {
	bytes := make([]byte, 16)
	rand.Read(bytes)
	return hex.EncodeToString(bytes)
}
