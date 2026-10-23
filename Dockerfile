# syntax=docker/dockerfile:1
FROM node:22-alpine AS web
WORKDIR /app/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1-alpine AS server
# cmake: libopus is built from its bundled source (the downloads' smaller copies, decision 0033).
RUN apk add --no-cache musl-dev build-base cmake
WORKDIR /app
COPY server/ server/
COPY --from=web /app/web/dist web/dist
WORKDIR /app/server
# The crates and what was compiled before are kept between builds (BuildKit caches): only what changed is built again.
RUN --mount=type=cache,id=cratarr-cargo-registry,target=/usr/local/cargo/registry     --mount=type=cache,id=cratarr-target,target=/app/server/target     cargo build --release --locked && cp target/release/cratarr-server /cratarr-server

FROM scratch
COPY --from=server /cratarr-server /cratarr-server
ENV CRATARR_BIND=0.0.0.0:8080
USER 1010:1001
EXPOSE 8080
# Five minutes to start before a failed check counts: a first start that moves a database (the analysis side table,
# 2026-10-17: about 4.5 min from the RAID) must not be marked unhealthy halfway.
HEALTHCHECK --interval=30s --timeout=5s --retries=3 --start-period=300s CMD ["/cratarr-server", "--healthcheck"]
ENTRYPOINT ["/cratarr-server"]
