# lightd Makefile
# Made by AI maker.
# Usage:
#   make build          - Build the binary
#   make run            - Run the daemon (default)
#   make serve          - Run the daemon explicitly
#   make network-list   - List Docker networks
#   make network-create - Create lightd network
#   make network-setup  - Setup complete lightd networking
#   make clean          - Clean build artifacts
#   make release        - Build optimized release binary
#   make install        - Install binary to /usr/local/bin
#   make test           - Run tests
#   make check          - Check code without building
#   make fmt            - Format code
#   make clippy         - Run clippy linter

# Variables
BINARY_NAME = lightd
TARGET_DIR = target
RELEASE_DIR = $(TARGET_DIR)/release
DEBUG_DIR = $(TARGET_DIR)/debug
INSTALL_PATH = /usr/local/bin

# Default arguments (can be overridden)
ARGS ?=
NETWORK_NAME ?= lightd-network
SUBNET ?= 172.20.0.0/16
GATEWAY ?= 172.20.0.1

# Default target
.PHONY: all
all: build

# Build targets
.PHONY: build
build:
	@echo "Building lightd..."
	cargo build

.PHONY: release
release:
	@echo "Building lightd (release)..."
	cargo build --release

.PHONY: check
check:
	@echo "🔍 Checking code..."
	cargo check

# Run targets
.PHONY: run
run: build
	@echo "Running lightd daemon..."
	./$(DEBUG_DIR)/$(BINARY_NAME) $(ARGS)

.PHONY: serve
serve: build
	@echo "🏃 Starting lightd daemon..."
	./$(DEBUG_DIR)/$(BINARY_NAME) serve $(ARGS)

.PHONY: run-release
run-release: release
	@echo "🏃 Running lightd daemon (release)..."
	./$(RELEASE_DIR)/$(BINARY_NAME) $(ARGS)

# Network management targets
.PHONY: network-list
network-list: build
	@echo "Listing Docker networks..."
	./$(DEBUG_DIR)/$(BINARY_NAME) network list

.PHONY: network-create
network-create: build
	@echo "🌐 Creating network: $(NETWORK_NAME)"
	./$(DEBUG_DIR)/$(BINARY_NAME) network create --name $(NETWORK_NAME) --subnet $(SUBNET) --gateway $(GATEWAY)

.PHONY: network-setup
network-setup: build
	@echo "Setting up lightd networking..."
	./$(DEBUG_DIR)/$(BINARY_NAME) network setup --name $(NETWORK_NAME) --subnet $(SUBNET) --gateway $(GATEWAY)

.PHONY: network-remove
network-remove: build
	@echo "Removing network: $(NETWORK_NAME)"
	./$(DEBUG_DIR)/$(BINARY_NAME) network remove --name $(NETWORK_NAME)

.PHONY: network-check
network-check: build
	@echo "Checking network: $(NETWORK_NAME)"
	./$(DEBUG_DIR)/$(BINARY_NAME) network check --name $(NETWORK_NAME)

# Custom network commands (pass arguments)
.PHONY: network
network: build
	@echo "🌐 Running network command: $(ARGS)"
	./$(DEBUG_DIR)/$(BINARY_NAME) network $(ARGS)

# Development targets
.PHONY: dev
dev:
	@echo "🔄 Running in development mode (auto-reload)..."
	cargo watch -x run

.PHONY: test
test:
	@echo "🧪 Running tests..."
	cargo test

.PHONY: fmt
fmt:
	@echo "🎨 Formatting code..."
	cargo fmt

.PHONY: clippy
clippy:
	@echo "📎 Running clippy..."
	cargo clippy -- -D warnings

# Installation targets
.PHONY: install
install: release
	@echo "📦 Installing lightd to $(INSTALL_PATH)..."
	sudo cp $(RELEASE_DIR)/$(BINARY_NAME) $(INSTALL_PATH)/
	@echo "lightd installed successfully!"

.PHONY: uninstall
uninstall:
	@echo "Uninstalling lightd from $(INSTALL_PATH)..."
	sudo rm -f $(INSTALL_PATH)/$(BINARY_NAME)
	@echo "lightd uninstalled successfully!"

# Cleanup targets
.PHONY: clean
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean

.PHONY: clean-all
clean-all: clean
	@echo "🧹 Cleaning all artifacts..."
	rm -rf $(TARGET_DIR)

# Docker targets
.PHONY: docker-build
docker-build:
	@echo "🐳 Building Docker image..."
	docker build -t lightd:latest .

.PHONY: docker-run
docker-run:
	@echo "🐳 Running lightd in Docker..."
	docker run --rm -p 3000:3000 -v /var/run/docker.sock:/var/run/docker.sock lightd:latest

# Utility targets
.PHONY: logs
logs:
	@echo "Showing lightd logs..."
	journalctl -u lightd -f

.PHONY: status
status:
	@echo "📊 Checking lightd status..."
	systemctl status lightd

.PHONY: restart
restart:
	@echo "🔄 Restarting lightd service..."
	sudo systemctl restart lightd

# Help target
.PHONY: help
help:
	@echo "lightd Makefile Commands:"
	@echo ""
	@echo "Build Commands:"
	@echo "  make build          - Build debug binary"
	@echo "  make release        - Build optimized release binary"
	@echo "  make check          - Check code without building"
	@echo ""
	@echo "Run Commands:"
	@echo "  make run            - Run daemon (default)"
	@echo "  make serve          - Run daemon explicitly"
	@echo "  make run-release    - Run release binary"
	@echo ""
	@echo "Network Commands:"
	@echo "  make network-list   - List Docker networks"
	@echo "  make network-create - Create lightd network"
	@echo "  make network-setup  - Setup complete networking"
	@echo "  make network-remove - Remove lightd network"
	@echo "  make network-check  - Check if network exists"
	@echo ""
	@echo "Custom Commands:"
	@echo "  make network ARGS='create --name custom --subnet 10.0.0.0/16'"
	@echo "  make run ARGS='--help'"
	@echo ""
	@echo "Development:"
	@echo "  make dev            - Run with auto-reload"
	@echo "  make test           - Run tests"
	@echo "  make fmt            - Format code"
	@echo "  make clippy         - Run linter"
	@echo ""
	@echo "Installation:"
	@echo "  make install        - Install to /usr/local/bin"
	@echo "  make uninstall      - Remove from /usr/local/bin"
	@echo ""
	@echo "Variables:"
	@echo "  NETWORK_NAME        - Network name (default: lightd-network)"
	@echo "  SUBNET              - Network subnet (default: 172.20.0.0/16)"
	@echo "  GATEWAY             - Network gateway (default: 172.20.0.1)"
	@echo "  ARGS                - Additional arguments"
	@echo ""
	@echo "Examples:"
	@echo "  make network-create NETWORK_NAME=my-net SUBNET=10.0.0.0/16"
	@echo "  make run ARGS='serve'"
	@echo "  make network ARGS='list'"

# Ensure target directories exist
$(DEBUG_DIR) $(RELEASE_DIR):
	mkdir -p $@