FROM rust:1.91-alpine AS builder

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
FROM alpine:3.21

RUN addgroup -g 1000 markdownlint \
    && adduser -D -u 1000 -G markdownlint markdownlint

USER markdownlint
WORKDIR /workspace

COPY --from=builder /mdlint /usr/local/bin/mdlint

ENTRYPOINT ["/usr/local/bin/mdlint"]
CMD ["--help"]
