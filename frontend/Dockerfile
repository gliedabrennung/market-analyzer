# `VITE_API_BASE_URL` (root README.md's "Конфигурация") is
# read by Vite at *build* time and inlined into the JS bundle — there is
# no runtime env var to override once this image is built. The default
# below matches docker-compose.yml's own backend port publish
# (localhost:8080, reachable from the browser on the host); override with
# --build-arg for any other topology (a real domain behind a reverse
# proxy, etc).
FROM node:22-bookworm-slim AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
ARG VITE_API_BASE_URL=http://localhost:8080
ENV VITE_API_BASE_URL=$VITE_API_BASE_URL
RUN npm run build

FROM nginx:1-alpine AS runtime
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
# 127.0.0.1, not localhost: this image's /etc/hosts resolves localhost to
# ::1 first, nginx only listens on 0.0.0.0:80, and busybox wget has no
# IPv4 fallback — with localhost every check fails with "connection
# refused" even though the server is fine (verified: `docker compose ps`
# showed the container "unhealthy" while `curl localhost:8081` from the
# host worked).
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 \
    CMD wget -qO- http://127.0.0.1/ >/dev/null || exit 1
