FROM node:22-bookworm-slim AS web
WORKDIR /web
RUN npm install -g pnpm@12
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build:static

FROM rust:bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p moraine-server

FROM debian:bookworm-slim
RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates curl \
	&& rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/moraine-server /usr/local/bin/moraine-server
COPY --from=web /web/build /srv/web
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["moraine-server"]
CMD ["--data-dir", "/data", "--bind", "0.0.0.0:8080", "--web-dir", "/srv/web", "--tls-terminated"]
