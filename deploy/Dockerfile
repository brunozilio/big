FROM oven/bun:1.3 AS web
WORKDIR /src/web
COPY web/package.json web/bun.lock web/bunfig.toml ./
RUN bun install --frozen-lockfile
COPY web/ ./
RUN bun run build

FROM rust:1.99-bookworm AS build
ARG COMMAND
ARG OVERLAY=deploy/shared.toml
WORKDIR /src
COPY . .
COPY --from=web /src/web/dist web/dist
ENV SKIP_WEB_BUILD=1 BRAND_OVERLAY=/src/${OVERLAY} CARGO_TARGET_DIR=/src/target/agents/image
RUN cargo build --release -p cli --bin ${COMMAND} && cp target/agents/image/release/${COMMAND} /app

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git ripgrep bash curl \
    && rm -rf /var/lib/apt/lists/* && useradd --create-home --uid 10001 app && mkdir -p /data && chown app /data
COPY --from=build /app /usr/local/bin/app
USER app
ENV HOME=/data XDG_CONFIG_HOME=/data
WORKDIR /data
EXPOSE 7420
ENTRYPOINT ["/usr/local/bin/app", "--no-open"]
