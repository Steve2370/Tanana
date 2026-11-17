# Relais de consultation à distance de Tanana.
# Construire depuis la racine du projet :
#   docker build -f crates/tanana-relais/Dockerfile -t tanana-relais .
FROM rust:1-bookworm AS construction
WORKDIR /src
COPY . .
RUN cargo build --release -p tanana-relais

FROM debian:bookworm-slim
RUN useradd --system --home /donnees relais && mkdir -p /donnees && chown relais /donnees
COPY --from=construction /src/target/release/tanana-relais /usr/local/bin/tanana-relais
USER relais
WORKDIR /donnees
ENV TANANA_RELAIS_ECOUTE=0.0.0.0:8080 \
    TANANA_RELAIS_BASE=/donnees/relais.db
EXPOSE 8080
VOLUME ["/donnees"]
CMD ["tanana-relais"]
