# Browser build + relay in one image: `papercraft-relay --static` serves web/ and the agent relay.
# craft-libs is a private Gitea dependency, so vendor on the host (authenticated) and pass it in:
#   cargo vendor --locked /path/vendor/crates >/dev/null
#   docker build -f deploy/Dockerfile --build-context vendor=/path/vendor -t localhost:32000/papercraft-web:<tag> .
FROM rust:slim AS build
# Must match the wasm-bindgen crate in Cargo.lock.
ARG WASM_BINDGEN=0.2.129
RUN rustup target add wasm32-unknown-unknown \
 && apt-get update && apt-get install -y --no-install-recommends binaryen pkg-config libssl-dev libdbus-1-dev \
 && rm -rf /var/lib/apt/lists/* \
 && cargo install wasm-bindgen-cli --version ${WASM_BINDGEN} --locked
COPY --from=vendor crates /vendor
RUN printf '%s\n' \
 '[source.crates-io]' 'replace-with = "vendored-sources"' \
 '[source."git+https://git.marcospaulo.dev.br/masi/craft-libs?rev=22544b6d619d4f16779b8bdfa3f0b642a86fc065"]' \
 'git = "https://git.marcospaulo.dev.br/masi/craft-libs"' \
 'rev = "22544b6d619d4f16779b8bdfa3f0b642a86fc065"' 'replace-with = "vendored-sources"' \
 '[source.vendored-sources]' 'directory = "/vendor"' > "$CARGO_HOME/config.toml"
WORKDIR /src
COPY . .
RUN sh web/build.sh && cargo build --release --locked --bin papercraft-relay

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends libssl3 libdbus-1-3 ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 papercraft
COPY --from=build /src/target/release/papercraft-relay /usr/local/bin/papercraft-relay
COPY --from=build /src/web/index.html /srv/web/index.html
COPY --from=build /src/web/pkg /srv/web/pkg
USER 10001
EXPOSE 8787
ENTRYPOINT ["papercraft-relay", "--static", "/srv/web", "--data", "/data"]
