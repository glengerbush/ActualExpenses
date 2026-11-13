# Build context: actualBudget/
FROM node:24.22.2-bookworm-slim@sha256:01d566e8d94d8c619591d78652babfccd3c6992a3dffa9ed5ab39e11cf6c70e9 AS web
WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run check && npm run build

FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS server
WORKDIR /build/server
COPY server/ ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/server/target \
    cargo build --locked --release && \
    install -Dm755 target/release/actualbudget-server /out/actualbudget-server && \
    strip /out/actualbudget-server

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
COPY --from=server /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=server /out/actualbudget-server /usr/local/bin/actualbudget-server
COPY --from=web /build/web/dist /app/web
RUN mkdir /data && chown 10001:10001 /data
ENV ACTUALBUDGET_DATA_DIR=/data \
    ACTUALBUDGET_WEB_DIR=/app/web \
    ACTUALBUDGET_HOST=0.0.0.0 \
    ACTUALBUDGET_PORT=5006
USER 10001:10001
VOLUME ["/data"]
EXPOSE 5006
ENTRYPOINT ["/usr/local/bin/actualbudget-server"]
