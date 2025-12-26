package network

import (
	"fmt"
	"net"
)

// IsTCPPortAvailable returns true if a TCP port can be bound on the local host.
// It attempts to bind to 0.0.0.0:port and immediately closes the listener if successful.
func IsTCPPortAvailable(port int) (bool, error) {
	addr := fmt.Sprintf(":%d", port)
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		// Port not available or other error
		return false, nil
	}
	_ = ln.Close()
	return true, nil
}
