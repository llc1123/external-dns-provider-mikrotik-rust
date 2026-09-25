FROM rust:1-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /src/target/release/external-dns-provider-mikrotik /usr/local/bin/external-dns-provider-mikrotik
USER 65532:65532
ENV SERVER_HOST=0.0.0.0
EXPOSE 8888 8080
ENTRYPOINT ["/usr/local/bin/external-dns-provider-mikrotik"]
