# syntax=docker/dockerfile:1
FROM node:24.22.0-bookworm-slim AS frontend
WORKDIR /build/frontend
RUN npm install --global pnpm@9.4.0
COPY package.json pnpm-lock.yaml .npmrc ./
RUN --mount=type=cache,id=sun-panel-pnpm,target=/pnpm/store \
    HUSKY=0 pnpm install --frozen-lockfile --store-dir=/pnpm/store
COPY index.html tsconfig.json vite.config.mts ./
COPY build ./build
COPY src ./src
COPY public ./public
ENV VITE_GLOB_API_URL=/api \
    VITE_GLOB_APP_PWA=false
RUN pnpm run build

FROM rust:1.99.1-trixie AS backend
WORKDIR /build/service-rust
ARG TARGETARCH
ENV RUSTUP_TOOLCHAIN=1.99.1
RUN apt-get update \
    && apt-get install --yes --no-install-recommends pkg-config cmake \
    && rm -rf /var/lib/apt/lists/*
COPY service-rust/Cargo.toml service-rust/Cargo.lock service-rust/rust-toolchain.toml ./
COPY service-rust/src ./src
COPY service-rust/migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,id=sun-panel-rust-target-${TARGETARCH},target=/build/service-rust/target \
    cargo build --release --locked \
    && cp target/release/sun-panel-rust /build/sun-panel-rust \
    && mkdir -p /build/runtime-data

FROM gcr.io/distroless/cc-debian13:nonroot@sha256:e792ab3d241a468a4fd7519ddbbebe66b49b5f365771716ea688ad40b6c6f1c2 AS runtime
ARG VERSION=1.0.0
LABEL org.opencontainers.image.title="Sun-Panel Rust" \
    org.opencontainers.image.description="Single-user navigation dashboard with Rust, Vue and SQLite" \
    org.opencontainers.image.version="${VERSION}" \
    org.opencontainers.image.licenses="MIT"
WORKDIR /app
COPY --from=backend --chown=10001:10001 /build/sun-panel-rust /app/sun-panel-rust
COPY --from=backend --chown=10001:10001 /build/runtime-data /app/data
COPY --from=frontend --chown=10001:10001 /build/frontend/dist /app/web
COPY LICENSE /app/LICENSE
COPY THIRD_PARTY_NOTICES.txt /app/THIRD_PARTY_NOTICES.txt
ENV SUN_PANEL_BIND=0.0.0.0:3002 \
    SUN_PANEL_DATA_DIR=/app/data \
    SUN_PANEL_WEB_DIR=/app/web \
    RUST_LOG=sun_panel_rust=info,tower_http=warn
USER 10001:10001
EXPOSE 3002
VOLUME ["/app/data"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/app/sun-panel-rust", "--healthcheck"]
CMD ["/app/sun-panel-rust"]
