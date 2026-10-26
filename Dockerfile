# syntax=docker/dockerfile:1

# ---------------------------------------------------------------------------
# One image: the SvelteKit app is built, then baked in next to the Rust binary
# that serves it. No node, no nginx, no second container at runtime.
# ---------------------------------------------------------------------------

FROM node:22-bookworm-slim AS web
WORKDIR /web

# Dependencies first so a source-only change does not reinstall them.
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

COPY frontend/ ./
# The build turns the release notes into the JSON the What's new page reads,
# and the script looks for them beside the frontend directory.
COPY RELEASE-NOTES.md /RELEASE-NOTES.md
RUN npm run build


FROM rust:1-bookworm AS api
WORKDIR /src

# Prime the dependency layer with a stub crate, so editing src/ does not
# rebuild every crate in the tree.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir -p src \
	&& echo 'fn main() {}' > src/main.rs \
	&& touch src/lib.rs \
	&& cargo build --release \
	&& rm -rf src

COPY backend/ ./
# Cargo keys off mtime, and the stub above is newer than the real sources.
RUN touch src/main.rs src/lib.rs && cargo build --release --locked


FROM debian:bookworm-slim AS runtime

RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates curl \
	&& rm -rf /var/lib/apt/lists/* \
	&& useradd --system --uid 10001 --create-home frog \
	&& mkdir -p /data && chown frog:frog /data

WORKDIR /app
COPY --from=api /src/target/release/frog /usr/local/bin/frog
COPY --from=web /web/build /app/static

ENV DATABASE_URL="sqlite:/data/frog.db?mode=rwc" \
	FROG_BIND="0.0.0.0:8080" \
	FROG_STATIC_DIR="/app/static" \
	FROG_REGION="BE" \
	FROG_LANGUAGE="en-US" \
	RUST_LOG="frog=info,tower_http=warn"

# The SQLite file, the TMDB cache and every cached poster live here. This is
# the only thing worth backing up.
VOLUME ["/data"]
EXPOSE 8080
USER frog

HEALTHCHECK --interval=30s --timeout=4s --start-period=10s --retries=3 \
	CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1

CMD ["frog"]
