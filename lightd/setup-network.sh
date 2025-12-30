#!/bin/bash

echo "lightd Network Setup"
echo "======================="

# Build the CLI tool
echo "Building lightd-network CLI..."
cargo build --bin lightd-network --release

if [ $? -ne 0 ]; then
    echo "❌ Failed to build lightd-network CLI"
    exit 1
fi

echo "✅ CLI built successfully"

# Check if Docker is running
if ! docker info >/dev/null 2>&1; then
    echo "❌ Docker is not running. Please start Docker first."
    exit 1
fi

echo "✅ Docker is running"

# Setup the network
echo ""
echo "Setting up lightd network..."
./target/release/lightd-network setup

if [ $? -eq 0 ]; then
    echo ""
    echo "lightd network setup complete!"
    echo ""
    echo "Available commands:"
    echo "  ./target/release/lightd-network list    # List all networks"
    echo "  ./target/release/lightd-network check   # Check if lightd network exists"
    echo "  ./target/release/lightd-network remove  # Remove lightd network"
    echo ""
    echo "You can now start the lightd daemon with: cargo run"
else
    echo "❌ Network setup failed"
    exit 1
fi