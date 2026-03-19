FROM ubuntu:22.04 AS builder
WORKDIR /build
COPY hello.txt /build/hello.txt

FROM alpine:3.18
COPY --from=builder /build/hello.txt /app/hello.txt
