FROM ubuntu:22.04
WORKDIR /app
COPY hello.txt /app/hello.txt
ENV GREETING=hi
RUN echo done
