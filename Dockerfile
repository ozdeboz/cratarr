# syntax=docker/dockerfile:1
# One build for testing and shipping (0.9.7, the owner): the tests run in the very stage the shipped binary is built in
# (`--target test`), the test server runs on that binary (`--target binary -o type=local,dest=<dir>`), and the image
# (the default target) copies it as built, from BuildKit's cache: nothing is compiled twice for one commit. The tools are
# pinned (tag and digest), so a commit builds the same web and the same binary every time. `CARGO_BUILD_JOBS` limits
# the compile on a machine short of memory (`--build-arg CARGO_BUILD_JOBS=1`); unset, every core.
FROM node:24.22.1-alpine@sha256:06d7adf3e0ef61a13f1d28682726366f97a15aa1888c934d3ecec86c33918b53 AS web
WORKDIR /app/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# The source and its toolchain, which the release build and the tests each start from (a test run no longer waits
# for the release build, which only the image needs).
FROM rust:1.99.0-alpine@sha256:0cce0a5e0e8ba67b455257a3a02a1d99005f382748789d6464460028810f1627 AS source
# cmake: libopus is built from its bundled source (the downloads' smaller copies, decision 0033).
RUN apk add --no-cache musl-dev build-base cmake
ARG CARGO_BUILD_JOBS
WORKDIR /app
COPY server/ server/
COPY --from=web /app/web/dist web/dist
WORKDIR /app/server

FROM source AS build
# The crates and what was compiled before are kept between builds (BuildKit caches): only what changed is built again.
RUN --mount=type=cache,id=cratarr-cargo-registry,target=/usr/local/cargo/registry     --mount=type=cache,id=cratarr-target,target=/app/server/target     cargo build --release --locked && cp target/release/cratarr-server /cratarr-server

# The tests, in the build's own toolchain and cache (the shipped binary's libc and allocator: musl's). The shared
# examples the web is held to too are read from ../web/tests/fixtures.
FROM source AS test
COPY web/tests/fixtures /app/web/tests/fixtures
COPY shared /app/shared
# Unoptimised (the owner, 2026-10-26): the tests are their own compile whatever is done, and this one is the quicker.
RUN --mount=type=cache,id=cratarr-cargo-registry,target=/usr/local/cargo/registry     --mount=type=cache,id=cratarr-target,target=/app/server/target     cargo test --locked --workspace

# The binary alone, to take out (the test server runs on it).
FROM scratch AS binary
COPY --from=build /cratarr-server /cratarr-server

FROM scratch
COPY --from=build /cratarr-server /cratarr-server
ENV CRATARR_BIND=0.0.0.0:8080
# Settings and databases in /data, mounted from outside; an install sets its own `user` (the owner of its music).
ENV CRATARR_DATA_DIR=/data
USER 1000:1000
EXPOSE 8080
# Five minutes to start before a failed check counts: a first start that moves a database (the analysis side table,
# 2026-10-17: about 4.5 min from the RAID) must not be marked unhealthy halfway.
HEALTHCHECK --interval=30s --timeout=5s --retries=3 --start-period=300s CMD ["/cratarr-server", "--healthcheck"]
ENTRYPOINT ["/cratarr-server"]
