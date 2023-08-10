FROM rust:alpine AS build

RUN apk add --update alpine-sdk

WORKDIR /src
COPY . /src

RUN cargo build --release

FROM alpine:latest

LABEL org.opencontainers.image.source=https://github.com/Linear-finance/reward-sender

COPY --from=build /src/target/release/reward-sender /usr/bin/

ENTRYPOINT [ "reward-sender" ]
