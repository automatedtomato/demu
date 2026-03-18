FROM scratch
WORKDIR /app
COPY nonexistent.txt /app/missing.txt
