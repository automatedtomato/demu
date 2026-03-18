FROM ubuntu:22.04

WORKDIR /app

ENV NODE_ENV=production
ENV PORT=3000
ENV APP_NAME=demu-demo

COPY tests/fixtures/integration/context/app.conf /app/config/app.conf
COPY tests/fixtures/integration/context/README.md /app/README.md

RUN apt-get update
RUN apt-get install -y curl wget git

WORKDIR /app/src

ENV DATABASE_URL=postgres://localhost:5432/demo
