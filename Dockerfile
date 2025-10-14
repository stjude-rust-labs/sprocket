FROM alpine:latest AS builder

# Install the necessary packages and Rust.
RUN apk add --update pkgconfig curl clang openssl-libs-static openssl-dev
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal

# Add `cargo` to the path.
ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /tmp/sprocket

# Copy the necessary source
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
COPY ./crates ./crates
COPY ./vendor ./vendor
COPY ./tests ./tests

# Build the release version of Sprocket
RUN cargo build --release

RUN strip target/release/sprocket

# Set up the final Sprocket image
FROM alpine:latest

RUN apk add --update shellcheck

COPY --from=builder /tmp/sprocket/target/release/sprocket /opt/sprocket/bin/sprocket

ENV PATH=/opt/sprocket/bin:$PATH

ENTRYPOINT ["sprocket"]
CMD ["--help"]
