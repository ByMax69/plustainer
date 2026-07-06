FROM rust:slim-bookworm AS builder

# Install dependencies for trunk and compilation
RUN apt-get update && apt-get install -y pkg-config libssl-dev wget curl

# Install trunk (pre-compiled binary for speed)
RUN wget -qO- https://github.com/trunk-rs/trunk/releases/download/v0.18.6/trunk-x86_64-unknown-linux-gnu.tar.gz | tar -xzf- -C /usr/local/bin

# Add Wasm target
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY . .

# Build frontend
WORKDIR /app/frontend
RUN trunk build --release

# Build backend
WORKDIR /app/backend
RUN cargo build --release

# Final runtime image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/plustainer-backend /app/plustainer-backend
COPY --from=builder /app/frontend/dist /app/public

EXPOSE 3456
ENV PORT=3456
CMD ["/app/plustainer-backend"]
