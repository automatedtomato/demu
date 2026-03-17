FROM ubuntu:22.04
WORKDIR /app
COPY . /app
ENV APP_ENV=production
RUN echo hello
