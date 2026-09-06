FROM rust:1.98-alpine@sha256:a10e64dd139b7387337c7fbe8aca31b959b57b2fd4c8ae20a02cf1d6ea424dce AS builder

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
