# syntax=docker/dockerfile:1.7

FROM node:22-bookworm AS admin-builder
WORKDIR /app/admin
COPY admin/package*.json ./
RUN npm ci
COPY admin/ ./
ARG VITE_API_BASE_URL=
ARG VITE_BSC_VAULT_CONTRACT=
ARG VITE_BASE_PATH=/admin/
ENV VITE_API_BASE_URL=${VITE_API_BASE_URL}
ENV VITE_BSC_VAULT_CONTRACT=${VITE_BSC_VAULT_CONTRACT}
ENV VITE_BASE_PATH=${VITE_BASE_PATH}
RUN npm run build

FROM node:22-bookworm AS dapp-builder
WORKDIR /app/dapp
COPY dapp/package*.json ./
RUN npm ci
COPY dapp/ ./
ARG VITE_API_BASE_URL=
ARG VITE_REOWN_PROJECT_ID=
ARG VITE_BASE_PATH=/dapp/
ENV VITE_API_BASE_URL=${VITE_API_BASE_URL}
ENV VITE_REOWN_PROJECT_ID=${VITE_REOWN_PROJECT_ID}
ENV VITE_BASE_PATH=${VITE_BASE_PATH}
RUN npm run build

FROM rust:1-bookworm AS backend-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=backend-builder /app/target/release/event-signal-prototype /usr/local/bin/event-signal-prototype
COPY migrations ./migrations
COPY --from=dapp-builder /app/dapp/dist ./dapp
COPY --from=admin-builder /app/admin/dist ./admin/dist
EXPOSE 8787
CMD ["event-signal-prototype", "--serve", "--listen", "0.0.0.0:8787"]
