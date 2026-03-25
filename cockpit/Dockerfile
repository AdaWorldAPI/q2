# Multi-stage Dockerfile for q2 Cockpit — deploys to Railway.com
# Builds the Vite/React app and serves with a static file server.

# ── Stage 1: Build ──────────────────────────────────────────────
FROM node:22-alpine AS build

WORKDIR /app

# Install dependencies first (layer cache)
COPY package.json ./
RUN npm install --ignore-scripts

# Copy source and build
COPY . .
RUN npm run build

# ── Stage 2: Serve ──────────────────────────────────────────────
FROM node:22-alpine AS runtime

WORKDIR /app

# Install a minimal static file server
RUN npm install -g serve@14

# Copy built assets from build stage
COPY --from=build /app/dist ./dist

# Railway injects PORT env var; default to 3000
ENV PORT=3000
EXPOSE 3000

# serve with SPA fallback (all routes -> index.html)
CMD sh -c "serve dist -l tcp://0.0.0.0:$PORT --single"
