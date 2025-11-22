#!/usr/bin/env bash
set -e

# Ensure cargo bin directory exists
mkdir -p "$HOME/.cargo/bin"

# Install rustup if not already installed
if ! command -v rustup &> /dev/null; then
  echo "Installing rustup..."
  curl https://sh.rustup.rs -sSf | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# Add required Rust targets
echo "Adding WebAssembly targets..."
rustup target add wasm32-unknown-unknown
# wasm32-wasip2 is often used for server-side wasm or other specific runtimes, keeping it as requested in the reference
rustup target add wasm32-wasip2

# Install cargo-binstall if missing
if ! command -v cargo-binstall &> /dev/null; then
  echo "Installing cargo-binstall..."
  curl -L https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-musl.tgz \
    | tar -xz -C "$HOME/.cargo/bin"
fi


# Install PostgreSQL system dependencies if on Debian/Ubuntu
if command -v apt-get &> /dev/null; then
  echo "Installing PostgreSQL and libpq-dev..."
  sudo apt-get update
  sudo apt-get install -y postgresql postgresql-contrib libpq-dev
else
  echo "Skipping system PostgreSQL installation (apt-get not found)."
fi

# Install sqlx-cli
if ! command -v sqlx &> /dev/null; then
    echo "Installing sqlx-cli..."
    cargo binstall -y sqlx-cli
else
    echo "sqlx-cli is already installed."
fi

echo "✅ Environment setup complete!"
