# Builds PokeChecks into a small image for hosting (Render reads this through render.yaml).

FROM rust:1.99-slim-trixie AS build
# reqwest's native-tls uses OpenSSL on Linux.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY assets assets
RUN cargo build --release --locked

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3t64 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home pokechecks
COPY --from=build /app/target/release/pokechecks /usr/local/bin/pokechecks
USER pokechecks
# Accept connections from outside the container. Hosts usually set PORT themselves.
ENV HOST=0.0.0.0 PORT=10000
EXPOSE 10000
CMD ["pokechecks"]
