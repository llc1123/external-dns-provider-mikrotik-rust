FROM rust:stable-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && useradd --system --create-home webhook
COPY --from=build /src/target/release/external-dns-provider-mikrotik /usr/local/bin/external-dns-provider-mikrotik
USER webhook
ENV SERVER_HOST=0.0.0.0
EXPOSE 8888 8080
ENTRYPOINT ["/usr/local/bin/external-dns-provider-mikrotik"]
