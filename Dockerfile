FROM rust:1.97-alpine@sha256:3c38f3f82c2f3d73da3b38e18d279393a04cb43ddded0e35088a8c3324d40900 AS builder

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
