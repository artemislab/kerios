# Multi-stage build for the kerios binary, suitable for the docker demo
# and for distribution as a Linux image.

FROM rust:1.85-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY kerios-core ./kerios-core
COPY kerios ./kerios
COPY packaging ./packaging
RUN cargo build --release -p kerios

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/kerios /usr/local/bin/kerios

# Make ~/.claude / ~/.codex exist so Provider::detect returns true and we
# don't need to baked-in volume init.
RUN mkdir -p /root/.claude /root/.codex /root/.kerios

ENV RUST_LOG=info
ENTRYPOINT ["/usr/local/bin/kerios"]
CMD ["daemon"]
