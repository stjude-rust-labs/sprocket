FROM rust:1.85 AS builder

WORKDIR /tmp/sprocket

COPY Cargo.lock Cargo.toml ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt update && apt install openssl ca-certificates -y && rm -rf /var/lib/apt/lists/*
COPY --from=builder /tmp/sprocket/target/release/sprocket /opt/sprocket/bin/sprocket

ENV PATH=/opt/sprocket/bin:$PATH

ENTRYPOINT ["sprocket"]
CMD ["--help"]
