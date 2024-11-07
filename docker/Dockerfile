####################################################################################################
## Builder
####################################################################################################
ARG RUST_VERSION=1.81.0
ARG APP_NAME=reka
FROM rust:${RUST_VERSION} AS builder
ARG APP_NAME
WORKDIR /app

RUN apt -y update \
    && apt -y upgrade \
    && apt install -y ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && update-ca-certificates

# Create appuser
ENV USER=app
ENV UID=10001

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    "${USER}"

# Build the application.
# Leverage a cache mount to /usr/local/cargo/registry/
# for downloaded dependencies and a cache mount to /app/target/ for 
# compiled dependencies which will speed up subsequent builds.
# Leverage a bind mount to the src directory to avoid having to copy the
# source code into the container. Once built, copy the executable to an
# output directory before the cache mounted /app/target is unmounted.
RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    <<EOF
set -e
cargo build --locked --release
strip -s /app/target/release/$APP_NAME
cp ./target/release/$APP_NAME /bin/$APP_NAME
EOF

####################################################################################################
## Final image
####################################################################################################
# FROM gcr.io/distroless/cc
FROM bitnami/minideb:bookworm

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates tzdata\
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Import from builder.
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group


WORKDIR /app

# Copy our build
COPY --from=builder /bin/reka ./
COPY config/base.yaml /app/config/base.yaml

# Use an unprivileged user.
USER app:app
ENV TZ=Etc/UTC
ENV APP_ENVIRONMENT=production
EXPOSE 8000

CMD ["/app/reka"]
