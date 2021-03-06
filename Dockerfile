FROM docker.io/rustlang/rust:nightly-alpine as build

WORKDIR /src

RUN apk add --no-cache musl-dev openssl-dev

COPY . .

RUN cargo build --release

FROM docker.io/alpine

WORKDIR /app

COPY --from=build /src/target/release/camera_bot .

ENTRYPOINT /app/camera_bot
