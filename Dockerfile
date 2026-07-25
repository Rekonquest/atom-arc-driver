FROM ubuntu:24.04 AS builder

ARG DEBIAN_FRONTEND=noninteractive
ARG RUST_VERSION=1.96.0

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}" \
        --component clippy,rustfmt

ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /source
COPY . .

RUN cargo fmt --all -- --check \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace \
    && cargo build --release -p atom-arcctl

FROM ubuntu:24.04
COPY --from=builder /source/target/release/atom-arcctl /usr/local/bin/atom-arcctl
ENTRYPOINT ["atom-arcctl"]
CMD ["probe"]
