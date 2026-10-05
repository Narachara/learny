# =============================================================================
# Stage 1 — Rust binary builder
# =============================================================================
FROM rust:bookworm AS builder

WORKDIR /build

# Cache dependency compilation: copy manifests first, then source.
# This layer is only invalidated when Cargo.toml / Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
COPY adapters/      adapters/
COPY application/   application/
COPY domain/        domain/
COPY ports/         ports/
COPY web/           web/
# Workspace members that must exist for `cargo build` to resolve the manifest
# but don't need source files since we're only compiling learny-web.
COPY ui/Cargo.toml      ui/Cargo.toml
COPY tauri/Cargo.toml   tauri/Cargo.toml
# Cargo needs at least one source file per workspace member to resolve targets.
RUN mkdir -p ui/src    && echo "fn main(){}" > ui/src/main.rs \
 && mkdir -p tauri/src && echo "" > tauri/src/lib.rs \
                       && echo "fn main(){}" > tauri/src/main.rs

RUN cargo build -p learny-web --release

# =============================================================================
# Stage 2 — Minimal runtime image
# =============================================================================
FROM debian:bookworm-slim AS runtime

# ca-certificates: needed for any outbound TLS calls (e.g. future integrations).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Binary
COPY --from=builder /build/target/release/learny-web /usr/local/bin/learny-web

# Pre-built WASM/JS/CSS — must exist before running `docker build`.
# Build it with: cd ui && dx build --no-default-features --features server --release
COPY target/dx/ui/release/web/public /app/dist

# Data volume — SQLite DB + uploaded files live here.
VOLUME /data

ENV DIST_DIR=/app/dist
ENV DATA_DIR=/data
ENV PORT=3000

EXPOSE 3000

CMD ["learny-web"]
