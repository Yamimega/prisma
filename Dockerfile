FROM rust:1-bookworm AS builder

WORKDIR /src
COPY . .

RUN cargo build --release -p prisma-cli

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/prisma /usr/local/bin/prisma

EXPOSE 8443/tcp 8443/udp 9090/tcp

ENTRYPOINT ["prisma"]
CMD ["server", "-c", "/config/server.toml"]
