FROM rust:1.85 AS builder

WORKDIR /tmp/sprocket

COPY Cargo.lock Cargo.toml ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm

COPY --from=builder /tmp/sprocket/target/release/sprocket /opt/sprocket/bin/sprocket

ENV PATH=/opt/sprocket/bin:$PATH

ENTRYPOINT ["sprocket"]
CMD ["--help"]