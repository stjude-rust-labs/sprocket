FROM rust:1.82 AS builder

WORKDIR /tmp/sprocket

COPY .git/ .git/
COPY Cargo.lock Cargo.toml ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm

COPY --from=builder /tmp/sprocket/target/release/sprocket /opt/sprocket/bin/sprocket

ENV PATH=/opt/sprocket/bin:$PATH

ENTRYPOINT ["/opt/sprocket/bin/sprocket"]
CMD ["--help"]