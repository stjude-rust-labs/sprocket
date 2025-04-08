FROM rust:1.85 AS builder

WORKDIR /tmp/sprocket

COPY Cargo.lock Cargo.toml ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm

COPY --from=builder /tmp/sprocket/target/release/sprocket /opt/sprocket/bin/sprocket

ENV PATH=/opt/sprocket/bin:$PATH

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl3 && \
    rm -rf /var/lib/apt/lists/*

ENTRYPOINT ["sprocket"]
CMD ["--help"]