FROM rust:alpine AS build

RUN apk add --update alpine-sdk

WORKDIR /src
COPY . /src

RUN cargo build --release

FROM alpine:latest

LABEL org.opencontainers.image.source=https://github.com/wadedesirco/test-project

COPY --from=build /src/target/release/test-project /usr/bin/

ENTRYPOINT [ "test-project" ]
