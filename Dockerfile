# Docker Hub official images mirrored on AWS Public ECR (reachable without a proxy).
FROM public.ecr.aws/docker/library/node:22-bookworm-slim AS ui
WORKDIR /src/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY ui/ ./
RUN npm run build

FROM public.ecr.aws/docker/library/rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=ui /src/ui/dist ./ui/dist
RUN cargo build -p photoself-server --release --features embed-ui

FROM public.ecr.aws/docker/library/debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/photoself /usr/local/bin/photoself
VOLUME ["/photos", "/data"]
EXPOSE 8787
ENTRYPOINT ["photoself"]
CMD ["serve", "-c", "/data/config.toml"]
