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
WORKDIR /src
COPY . .
# The craft-libs revision is read from Cargo.lock, so the image never pins a stale one.
RUN rev=$(grep -o 'craft-libs?rev=[0-9a-f]*' Cargo.lock | head -n1 | cut -d= -f2) \
 && test -n "$rev" \
 && printf '%s\n' \
 '[source.crates-io]' 'replace-with = "vendored-sources"' \
 "[source.\"git+https://git.marcospaulo.dev.br/masi/craft-libs?rev=$rev\"]" \
 'git = "https://git.marcospaulo.dev.br/masi/craft-libs"' \
 "rev = \"$rev\"" 'replace-with = "vendored-sources"' \
 '[source.vendored-sources]' 'directory = "/vendor"' > "$CARGO_HOME/config.toml"
RUN sh web/build.sh && cargo build --release --locked --bin papercraft-relay

FROM debian:stable-slim
# Node and the two agent CLIs `--agents` can start, at exact versions (bump them on purpose).
ARG CLAUDE_CODE=2.1.319
ARG OPENCODE=1.19.5
RUN apt-get update && apt-get install -y --no-install-recommends libssl3 libdbus-1-3 ca-certificates nodejs npm \
 && npm install -g --no-audit --no-fund @anthropic-ai/claude-code@${CLAUDE_CODE} opencode-ai@${OPENCODE} \
 && npm cache clean --force \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 papercraft
ENV DISABLE_AUTOUPDATER=1
COPY --from=build /src/target/release/papercraft-relay /usr/local/bin/papercraft-relay
COPY --from=build /src/web/index.html /src/web/manifest.webmanifest /srv/web/
COPY --from=build /src/web/pkg /srv/web/pkg
COPY --from=build /src/web/icons /srv/web/icons
USER 10001
EXPOSE 8787
ENTRYPOINT ["papercraft-relay", "--static", "/srv/web", "--data", "/data"]
