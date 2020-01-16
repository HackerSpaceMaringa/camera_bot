FROM rust:alpine as build

WORKDIR /src

RUN apk add --no-cache musl-dev openssl-dev

COPY Cargo.* ./

RUN cargo update --locked

COPY . .

ENV RUSTFLAGS "-C target-feature=-crt-static"
RUN cargo build --release

FROM alpine

RUN apk add --no-cache libgcc openssl

WORKDIR /app

COPY --from=build /src/target/release/camera_bot .

ENTRYPOINT /app/camera_bot
