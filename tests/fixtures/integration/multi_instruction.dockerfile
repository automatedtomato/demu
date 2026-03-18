FROM node:20-slim
WORKDIR /srv
ENV NODE_ENV=production
COPY app.conf /srv/app.conf
RUN npm install
COPY README.md /srv/README.md
ENV PORT=8080
