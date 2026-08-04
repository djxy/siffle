FROM rust:1.97-slim AS chef

RUN cargo install --locked cargo-chef

WORKDIR /app

FROM chef AS planner

COPY . .

RUN cargo chef prepare  --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release --bin siffle

FROM gcr.io/distroless/cc-debian13

COPY --from=builder /app/target/release/siffle /usr/local/bin/siffle

USER nonroot:nonroot

ENTRYPOINT ["/usr/local/bin/siffle"]
