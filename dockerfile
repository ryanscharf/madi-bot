# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install libpdfium.so (used by pdf_availability to render/crop game notes
# PDFs) into the standard library path so it's found automatically.
RUN curl -sL https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8057/pdfium-linux-x64.tgz \
    -o /tmp/pdfium.tgz \
    && tar -xzf /tmp/pdfium.tgz -C /tmp \
    && cp /tmp/lib/libpdfium.so /usr/lib/libpdfium.so \
    && ldconfig \
    && rm -rf /tmp/pdfium.tgz /tmp/lib /tmp/include \
    && apt-get purge -y curl \
    && apt-get autoremove -y

# Copy the binary from builder
COPY --from=builder /app/target/release/madi-bot /usr/local/bin/madi-bot

# Run the bot
CMD ["madi-bot"]