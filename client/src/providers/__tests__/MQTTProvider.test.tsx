/**
 * MQTT Provider Tests
 */
import React from 'react';
import { render, screen, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { MQTTProvider, useMQTT, useTenantUpdates } from '../MQTTProvider';
import { useAuth } from '../AuthProvider';

// Mock MQTT client
const mockMqttClient = {
  on: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  publish: vi.fn(),
  end: vi.fn(),
  options: { clientId: 'test-client' }
};

// Mock mqtt module
vi.mock('mqtt', () => ({
  connect: vi.fn(() => mockMqttClient)
}));

// Mock AuthProvider
vi.mock('../AuthProvider', () => ({
  useAuth: vi.fn()
}));

// Test component that uses MQTT
function TestMQTTComponent() {
  const mqtt = useMQTT();
  return (
    <div>
      <div data-testid="connection-status">
        {mqtt.isConnected ? 'Connected' : 'Disconnected'}
      </div>
    </div>
  );
}

// Test component that uses tenant updates
function TestTenantUpdatesComponent({ tenantId }: { tenantId: string }) {
  const updates = useTenantUpdates(tenantId);
  return (
    <div>
      <div data-testid="updates-count">{updates.length}</div>
      {updates.map((update, index) => (
        <div key={index} data-testid={`update-${index}`}>
          {update.type}: {JSON.stringify(update.data)}
        </div>
      ))}
    </div>
  );
}

describe('MQTTProvider', () => {
  const mockConfig = {
    url: 'ws://localhost:4001/mqtt',
    username: 'test-user',
    password: 'test-password',
    encryptionKey: '64-char-hex-encryption-key-for-testing-purposes-only-not-secure',
    signingKey: '64-char-hex-signing-key-for-testing-purposes-only-not-secure'
  };

  const mockUser = {
    id: 'user-123',
    email: 'test@example.com',
    username: 'testuser'
  };

  beforeEach(() => {
    vi.clearAllMocks();
    
    // Mock useAuth to return authenticated user
    (useAuth as any).mockReturnValue({
      user: mockUser,
      token: 'test-token'
    });

    // Reset MQTT client mock
    mockMqttClient.on.mockImplementation((event, callback) => {
      if (event === 'connect') {
        // Simulate immediate connection
        setTimeout(() => callback(), 0);
      }
      return mockMqttClient;
    });

    mockMqttClient.subscribe.mockImplementation((topic, options, callback) => {
      if (typeof options === 'function') {
        setTimeout(() => options(null), 0);
      } else if (callback) {
        setTimeout(() => callback(null), 0);
      }
      return mockMqttClient;
    });
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it('should provide MQTT context when config is available', async () => {
    render(
      <MQTTProvider config={mockConfig}>
        <TestMQTTComponent />
      </MQTTProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('connection-status')).toHaveTextContent('Connected');
    });
  });

  it('should not connect when no config is provided', () => {
    render(
      <MQTTProvider config={null}>
        <TestMQTTComponent />
      </MQTTProvider>
    );

    expect(screen.getByTestId('connection-status')).toHaveTextContent('Disconnected');
  });

  it('should not connect when user is not authenticated', () => {
    (useAuth as any).mockReturnValue({
      user: null,
      token: null
    });

    render(
      <MQTTProvider config={mockConfig}>
        <TestMQTTComponent />
      </MQTTProvider>
    );

    expect(screen.getByTestId('connection-status')).toHaveTextContent('Disconnected');
  });

  it('should handle tenant updates subscription', async () => {
    const tenantId = 'tenant-123';

    render(
      <MQTTProvider config={mockConfig}>
        <TestTenantUpdatesComponent tenantId={tenantId} />
      </MQTTProvider>
    );

    await waitFor(() => {
      // Verify subscription calls
      expect(mockMqttClient.subscribe).toHaveBeenCalledWith(
        `heliactyl/tenant/${tenantId}/servers/updates`,
        { qos: 1 },
        expect.any(Function)
      );
      expect(mockMqttClient.subscribe).toHaveBeenCalledWith(
        `heliactyl/tenant/${tenantId}/resources/updates`,
        { qos: 1 },
        expect.any(Function)
      );
      expect(mockMqttClient.subscribe).toHaveBeenCalledWith(
        `heliactyl/tenant/${tenantId}/members/updates`,
        { qos: 1 },
        expect.any(Function)
      );
    });

    expect(screen.getByTestId('updates-count')).toHaveTextContent('0');
  });

  it('should process incoming MQTT messages', async () => {
    const tenantId = 'tenant-123';
    let messageHandler: ((topic: string, payload: Buffer) => void) | undefined;

    // Capture the message handler
    mockMqttClient.on.mockImplementation((event, callback) => {
      if (event === 'connect') {
        setTimeout(() => callback(), 0);
      } else if (event === 'message') {
        messageHandler = callback;
      }
      return mockMqttClient;
    });

    render(
      <MQTTProvider config={mockConfig}>
        <TestTenantUpdatesComponent tenantId={tenantId} />
      </MQTTProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('connection-status')).toBeDefined();
    });

    // Simulate receiving a message
    if (messageHandler) {
      const mockMessage = {
        type: 'server_created',
        timestamp: new Date().toISOString(),
        data: {
          encrypted: btoa(JSON.stringify({ id: 'server-456', name: 'Test Server' })),
          iv: 'mock-iv',
          tag: 'mock-tag'
        },
        signature: 'mock-signature',
        tenantId: tenantId
      };

      act(() => {
        messageHandler!(
          `heliactyl/tenant/${tenantId}/servers/updates`,
          Buffer.from(JSON.stringify(mockMessage))
        );
      });

      await waitFor(() => {
        expect(screen.getByTestId('updates-count')).toHaveTextContent('1');
        expect(screen.getByTestId('update-0')).toHaveTextContent('server_created');
      });
    }
  });

  it('should unsubscribe when component unmounts', async () => {
    const tenantId = 'tenant-123';

    const { unmount } = render(
      <MQTTProvider config={mockConfig}>
        <TestTenantUpdatesComponent tenantId={tenantId} />
      </MQTTProvider>
    );

    await waitFor(() => {
      expect(mockMqttClient.subscribe).toHaveBeenCalled();
    });

    unmount();

    await waitFor(() => {
      expect(mockMqttClient.unsubscribe).toHaveBeenCalledWith(
        `heliactyl/tenant/${tenantId}/servers/updates`,
        expect.any(Function)
      );
    });
  });

  it('should handle connection errors gracefully', async () => {
    // Mock connection error
    mockMqttClient.on.mockImplementation((event, callback) => {
      if (event === 'error') {
        setTimeout(() => callback(new Error('Connection failed')), 0);
      }
      return mockMqttClient;
    });

    // Mock console.error to prevent error output in tests
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    render(
      <MQTTProvider config={mockConfig}>
        <TestMQTTComponent />
      </MQTTProvider>
    );

    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith(
        '[HeliaMQTT] Client error:',
        expect.any(Error)
      );
    });

    consoleSpy.mockRestore();
  });

  it('should validate message signatures', async () => {
    const tenantId = 'tenant-123';
    let messageHandler: ((topic: string, payload: Buffer) => void) | undefined;

    mockMqttClient.on.mockImplementation((event, callback) => {
      if (event === 'connect') {
        setTimeout(() => callback(), 0);
      } else if (event === 'message') {
        messageHandler = callback;
      }
      return mockMqttClient;
    });

    const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

    render(
      <MQTTProvider config={mockConfig}>
        <TestTenantUpdatesComponent tenantId={tenantId} />
      </MQTTProvider>
    );

    await waitFor(() => {
      expect(messageHandler).toBeDefined();
    });

    // Send message with invalid signature
    if (messageHandler) {
      const invalidMessage = {
        type: 'server_created',
        timestamp: new Date().toISOString(),
        data: { encrypted: 'test', iv: 'test', tag: 'test' },
        signature: '', // Invalid signature
        tenantId: tenantId
      };

      act(() => {
        messageHandler!(
          `heliactyl/tenant/${tenantId}/servers/updates`,
          Buffer.from(JSON.stringify(invalidMessage))
        );
      });

      await waitFor(() => {
        expect(consoleSpy).toHaveBeenCalledWith(
          '[HeliaMQTT] Invalid message signature, ignoring'
        );
        expect(screen.getByTestId('updates-count')).toHaveTextContent('0');
      });
    }

    consoleSpy.mockRestore();
  });
});

describe('useMQTT hook', () => {
  it('should throw error when used outside provider', () => {
    const TestComponent = () => {
      useMQTT();
      return <div>Test</div>;
    };

    expect(() => render(<TestComponent />)).toThrow(
      'useMQTT must be used within an MQTTProvider'
    );
  });
});

describe('useTenantUpdates hook', () => {
  it('should handle null tenantId', () => {
    const TestComponent = () => {
      const updates = useTenantUpdates(null);
      return <div data-testid="updates-count">{updates.length}</div>;
    };

    render(
      <MQTTProvider config={null}>
        <TestComponent />
      </MQTTProvider>
    );

    expect(screen.getByTestId('updates-count')).toHaveTextContent('0');
  });
});