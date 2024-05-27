# Use a slim Ubuntu image for the builder stage
FROM alpine:latest as builder

# Install necessary dependencies
RUN apk update && apk add --no-cache \
    curl \
    build-base \
    openssl-dev

# Install Rust 1.80 nightly
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2024-05-07
ENV PATH="/root/.cargo/bin:${PATH}"

# Create a new directory for the Rust project
WORKDIR /app

# Copy the source files directly
COPY . .

# Build the Rust binary
RUN cargo build --release

# Use a slim Ubuntu image for the runtime stage
FROM alpine:latest

# Install necessary runtime libraries
RUN apk update && apk add --no-cache \
    libssl1.1

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/chainbridge-monitor /usr/local/bin/chainbridge-monitor