FROM rust:1.88 AS builder

# Install Sprocket by invoking `cargo install` on the sources in the current directory. The `mount`
# directive provides the current directory to the `builder` container, so no unnecessary copying is
# performed into `builder`.
WORKDIR /tmp/sprocket
RUN --mount=type=bind,source=.,target=/tmp/sprocket,readonly \
    cargo install --target-dir /tmp/sprocket-target --root /tmp/sprocket-root --path .

FROM debian:bookworm-slim

RUN apt update && apt install -y openssl ca-certificates shellcheck && rm -rf /var/lib/apt/lists/*
COPY --from=builder /tmp/sprocket-root/bin/sprocket /opt/sprocket/bin/sprocket

ENV PATH=/opt/sprocket/bin:$PATH

ENTRYPOINT ["sprocket"]
CMD ["--help"]
