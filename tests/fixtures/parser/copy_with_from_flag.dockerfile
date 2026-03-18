FROM scratch
COPY --from=builder /out/app /app/app
