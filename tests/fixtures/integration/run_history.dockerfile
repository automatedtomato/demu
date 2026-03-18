FROM debian:bullseye
RUN apt-get update
RUN apt-get install -y curl wget
RUN echo "setup complete"
