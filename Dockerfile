FROM rust:1.82

RUN cargo install sprocket

ENTRYPOINT ["sprocket"]
CMD ["--help"]