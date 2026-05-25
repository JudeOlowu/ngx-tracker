FROM rust:1.85-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release || true && rm -f src/main.rs
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/ngx_screener .

# Debug: show what libraries the binary needs
RUN ldd ./ngx_screener

ENV PORT=3000
ENV RUST_LOG=debug
EXPOSE $PORT
CMD ["./ngx_screener", "--serve"]
