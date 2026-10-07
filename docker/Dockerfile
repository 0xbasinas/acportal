FROM rust:1.96-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY protocol ./protocol
COPY acpd ./acpd
RUN cargo build --locked --release -p acpd --bin acpd

FROM debian:bookworm-slim
RUN useradd --create-home --uid 10001 acpd
COPY --from=build /src/target/release/acpd /usr/local/bin/acpd
USER acpd
WORKDIR /home/acpd
ENTRYPOINT ["acpd"]
CMD ["--help"]
