/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MEDIATAGGER_PERF?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
