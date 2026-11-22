FROM node:24.20.0-bookworm-slim AS dependencies
WORKDIR /broker
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund

FROM node:24.20.0-bookworm-slim AS runtime
ENV NODE_ENV=production \
    BROKER_MODE=local-synthetic \
    HOST=127.0.0.1 \
    PORT=8080
WORKDIR /broker
COPY --from=dependencies /broker/node_modules ./node_modules
COPY package.json package-lock.json ./
COPY src ./src
USER node
CMD ["node", "src/server.mjs"]
