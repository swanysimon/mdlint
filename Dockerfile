FROM rust:1.98-alpine@sha256:7cc1c22d77d9432f7fe012a70e6d3e555af54c2a6832700ed7d553f1769ae89f AS builder

# Docker provides these automatically based on --platform
ARG TARGETARCH

RUN apk add --no-cache musl-dev

# Set Rust target based on architecture and target for static linking
RUN case "${TARGETARCH}" in \
        amd64) echo "x86_64-unknown-linux-musl" > /rust-target ;; \
        arm64) echo "aarch64-unknown-linux-musl" > /rust-target ;; \
        *) echo "Unsupported architecture: ${TARGETARCH}" && exit 1 ;; \
    esac \
    && rustup target add "$(cat /rust-target)"

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --target "$(cat /rust-target)" \
    && mv "target/$(cat /rust-target)/release/mdlint" /mdlint

# Runtime stage
FROM scratch

COPY --from=builder /mdlint /usr/local/bin/mdlint

ENTRYPOINT ["/usr/local/bin/mdlint"]
CMD ["--help"]
