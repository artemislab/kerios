FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
EXPOSE 9418
ENTRYPOINT ["git", "daemon", "--base-path=/srv", "--export-all", "--reuseaddr", "--enable=receive-pack", "--verbose", "--listen=0.0.0.0"]
