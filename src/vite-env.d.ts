/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Adresse du serveur de consultation à distance proposée par défaut (ex. https://relais.exemple.com). */
  readonly VITE_TANANA_RELAIS?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
