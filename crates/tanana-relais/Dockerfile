# Relais de consultation à distance de Tanana, avec sa page web.
# Construire depuis la racine du projet :
#   docker build -f crates/tanana-relais/Dockerfile -t tanana-relais .

# 1. Page web de consultation (déchiffre tout dans le navigateur)
FROM node:22-bookworm-slim AS page
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund
COPY . .
RUN npm run build:web

# 2. Relais
FROM rust:1-bookworm AS construction
WORKDIR /src
COPY . .
RUN cargo build --release -p tanana-relais

FROM debian:bookworm-slim
RUN useradd --system --home /donnees relais && mkdir -p /donnees && chown relais /donnees
COPY --from=construction /src/target/release/tanana-relais /usr/local/bin/tanana-relais
COPY --from=page /src/dist-web /srv/consultation
USER relais
WORKDIR /donnees
ENV TANANA_RELAIS_ECOUTE=0.0.0.0:8080 \
    TANANA_RELAIS_BASE=/donnees/relais.db \
    TANANA_RELAIS_PAGES=/srv/consultation
EXPOSE 8080
VOLUME ["/donnees"]
CMD ["tanana-relais"]
