# syntax=docker/dockerfile:1.7
#
# Multi-stage build:
#   1. web: bun builds the SvelteKit SPA into /web/build
#   2. chef + builder: cargo-chef caches the Rust dep layer; the Rust build
#      embeds /web/build via rust-embed during compilation
#   3. runtime: distroless cc, single static-ish binary, non-root user
#
# Final image lands well under the 100MB target (cc-debian12 is ~25MB; the
# stripped errexd binary plus the embedded SPA is the rest).

# ---------- web: build the SPA with bun ----------
FROM oven/bun:1-slim AS web-builder
WORKDIR /web

# Set SKIP_WEB_BUILD=true if you've already run `bun run build` on the host
# and just want docker to pick up the existing `web/build` artifacts. Useful
# when Docker Desktop's engine RAM is too small for vite's SSR pass on
# large component graphs (the build legitimately wants ~3 GB peak).
ARG SKIP_WEB_BUILD=false

# Copy the whole web/ subtree in one shot — bun 1.2 ships a text lockfile
# (`bun.lock`), but older versions used `bun.lockb`, and the legacy Docker
# builder errors on a glob with no match. Copying the directory dodges both
# concerns; the install layer is invalidated only on package.json/lockfile
# changes thanks to BuildKit's content hashing (or the legacy builder's
# whole-context hash, which is fine for our scale).
COPY web/ ./
RUN if [ "$SKIP_WEB_BUILD" = "true" ] && [ -d build ]; then \
      echo "SKIP_WEB_BUILD=true: using prebuilt web/build from the host"; \
    else \
      bun install --frozen-lockfile --no-progress \
   && bun run build; \
    fi

# ---------- chef: shared base with cargo-chef installed ----------
FROM rust:1-slim AS chef
RUN cargo install --locked cargo-chef --version ^0.1
WORKDIR /build

# ---------- planner: compute the dep recipe ----------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- builder: cook deps, then build with embedded SPA ----------
FROM chef AS builder
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*

COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --bin errexd

COPY . .
# rust-embed reads $CARGO_MANIFEST_DIR/../../web/build/ at compile time, so
# the SPA must be in place before cargo runs.
COPY --from=web-builder /web/build /build/web/build
RUN cargo build --release --bin errexd \
 && strip target/release/errexd

# Stage out the user/group entries and the data dir into a scratch tree the
# distroless runtime can COPY in (distroless has no shell, so we can't
# `useradd` there). We use `--chown=1000:1000` on the COPYs so /data stays
# writable to the runtime user.
FROM debian:stable-slim AS rootfs
RUN mkdir -p /pkg/data /pkg/home/errex \
 && echo 'errex:x:1000:1000::/home/errex:/sbin/nologin' > /pkg/passwd \
 && echo 'errex:x:1000:' > /pkg/group

# ---------- runtime: distroless cc, non-root, single static-ish binary ----------
FROM gcr.io/distroless/cc-debian12 AS runtime
COPY --from=rootfs /pkg/passwd /etc/passwd
COPY --from=rootfs /pkg/group /etc/group
COPY --from=rootfs --chown=1000:1000 /pkg/home/errex /home/errex
COPY --from=rootfs --chown=1000:1000 /pkg/data /data
COPY --from=builder /build/target/release/errexd /usr/local/bin/errexd

USER 1000
WORKDIR /home/errex
VOLUME ["/data"]
EXPOSE 9090 9091 9092

ENV ERREX_DATA_DIR=/data \
    ERREX_LOG_LEVEL=info

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/errexd", "healthcheck"]

ENTRYPOINT ["/usr/local/bin/errexd"]
