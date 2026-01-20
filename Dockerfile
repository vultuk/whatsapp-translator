# Build stage for Go bridge
FROM golang:1.24-bookworm AS go-builder

WORKDIR /build
COPY wa-bridge/ ./
RUN go mod download && \
    CGO_ENABLED=0 go build -ldflags="-s -w" -o wa-bridge .

# Build stage for Rust application
FROM rust:1.84-bookworm AS rust-builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY build.rs ./

# Skip Go build in Rust build script (we build it separately)
ENV SKIP_GO_BUILD=1

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 app

# Create data directory
RUN mkdir -p /data && chown app:app /data

WORKDIR /app

# Copy binaries from build stages
COPY --from=go-builder /build/wa-bridge /app/wa-bridge
COPY --from=rust-builder /build/target/release/whatsapp-translator /app/whatsapp-translator

# Copy web frontend
COPY web/public/ /app/web/public/

# Make binaries executable
RUN chmod +x /app/wa-bridge /app/whatsapp-translator

# Set ownership
RUN chown -R app:app /app

USER app

# Environment variables with defaults
# WA_VERBOSE - Enable verbose logging (true/false)
# WA_JSON - Output as JSON (true/false)
# WA_LOGOUT - Clear session on start (true/false)
# WA_WEB - Enable web server mode (true/false)
# WA_PORT - Web server port (default: 3000)
# WA_HOST - Web server host (default: 0.0.0.0)
# WA_DEFAULT_LANGUAGE - Default language (default: English)
# ANTHROPIC_API_KEY - Claude API key for translation

ENV WA_DATA_DIR=/data \
    WA_BRIDGE_PATH=/app/wa-bridge \
    WA_WEB=true \
    WA_PORT=3000 \
    WA_HOST=0.0.0.0

# Expose web server port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:${WA_PORT}/api/status || exit 1

# Note: For persistent data (WhatsApp session), mount a volume to /data
# On Railway, use Railway Volumes: https://docs.railway.com/reference/volumes

ENTRYPOINT ["/app/whatsapp-translator"]
