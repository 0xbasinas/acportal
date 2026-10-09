FROM rust:1.96-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY protocol ./protocol
COPY acpd ./acpd
RUN cargo build --locked --release -p acpd --bin acpd

FROM debian:bookworm-slim AS runtime
RUN useradd --create-home --uid 10001 acpd
COPY --from=build /src/target/release/acpd /usr/local/bin/acpd
USER acpd
WORKDIR /home/acpd
ENTRYPOINT ["acpd"]
CMD ["--help"]

# Test-only image (`docker build --target smoke`): the host plus the repository's mock
# agent, used by scripts/container-smoke.sh. The default image below does not contain it.
FROM build AS mock-build
RUN cargo build --locked --release -p acpd --bin mock-acp-agent

FROM runtime AS smoke
COPY --from=mock-build /src/target/release/mock-acp-agent /usr/local/bin/mock-acp-agent

# Default (production) image: the last stage, identical to `runtime`.
FROM runtime
