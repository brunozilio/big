FROM oven/bun:1.4.4 AS web
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
ENV SKIP_WEB_BUILD=1 BRAND_OVERLAY=/src/${OVERLAY} CARGO_TARGET_DIR=/cache/target
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=image-target,target=/cache/target \
    cargo build --profile image -p cli --bin ${COMMAND} && cp /cache/target/image/${COMMAND} /app
RUN --mount=type=secret,id=logic_key \
    mkdir -p /logic && if [ -s /run/secrets/logic_key ]; then /app sign-logic /run/secrets/logic_key /logic; fi

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git ripgrep bash curl \
    && rm -rf /var/lib/apt/lists/* && useradd --create-home --uid 10001 app && mkdir -p /data && chown app /data \
    && curl -fsSLo /usr/local/share/rds-ca.pem https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem
COPY --from=build /app /usr/local/bin/app
COPY --from=build /logic /usr/local/share/logic
ENV HOME=/data XDG_CONFIG_HOME=/data
WORKDIR /data
EXPOSE 7420
ENTRYPOINT ["/usr/local/bin/app", "--no-open"]
