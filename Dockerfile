FROM rust:bookworm AS build
WORKDIR /src
COPY . .
RUN rustup target add wasm32-unknown-unknown \
	&& cargo install wasm-pack --locked \
	&& cd wasm \
	&& wasm-pack build --target web --out-dir ../web/src/lib/signer/pkg --out-name moraine_wasm
RUN cargo build --release -p moraine-server

FROM node:22-bookworm-slim AS web
WORKDIR /src
RUN npm install -g pnpm@12
COPY . .
COPY --from=build /src/web/src/lib/signer/pkg ./web/src/lib/signer/pkg
WORKDIR /src/web
ARG PUBLIC_MORAINE_REGISTRY=
RUN printf 'PUBLIC_MORAINE_REGISTRY=%s\n' "$PUBLIC_MORAINE_REGISTRY" > .env
RUN pnpm install --frozen-lockfile
RUN MORAINE_WEB_TARGET=static pnpm exec vite build

FROM debian:bookworm-slim
RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates curl \
	&& rm -rf /var/lib/apt/lists/* \
	&& useradd --system --uid 10001 --create-home moraine \
	&& mkdir -p /data \
	&& chown moraine:moraine /data
COPY --from=build /src/target/release/moraine-server /usr/local/bin/moraine-server
COPY --from=web /src/web/build /srv/web
VOLUME /data
USER moraine
EXPOSE 8080
ENTRYPOINT ["moraine-server"]
CMD ["--data-dir", "/data", "--bind", "0.0.0.0:8080", "--web-dir", "/srv/web", "--tls-terminated"]
