FROM node:22-slim AS dashboard
WORKDIR /dashboard
COPY prisma-dashboard/package.json prisma-dashboard/package-lock.json ./
RUN npm ci
COPY prisma-dashboard/ ./
RUN npm run build

FROM rust:1-bookworm AS builder
WORKDIR /src

# Stage 1: cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY prisma-core/Cargo.toml prisma-core/
COPY prisma-server/Cargo.toml prisma-server/
COPY prisma-client/Cargo.toml prisma-client/
COPY prisma-cli/Cargo.toml prisma-cli/
COPY prisma-mgmt/Cargo.toml prisma-mgmt/
RUN mkdir -p prisma-core/src prisma-server/src prisma-client/src prisma-mgmt/src \
    && echo "fn main(){}" > prisma-cli/src/main.rs \
    && touch prisma-core/src/lib.rs prisma-server/src/lib.rs \
              prisma-client/src/lib.rs prisma-mgmt/src/lib.rs \
    && cargo build --release -p prisma-cli 2>/dev/null || true

# Stage 2: build actual code (deps are cached)
COPY . .
RUN cargo build --release -p prisma-cli

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/prisma /usr/local/bin/prisma
COPY --from=dashboard /dashboard/out /opt/prisma/dashboard

EXPOSE 8443/tcp 8443/udp 9090/tcp

ENTRYPOINT ["prisma"]
CMD ["server", "-c", "/config/server.toml"]
