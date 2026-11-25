import { defineConfig } from "vitest/config";

// Tests à la racine du projet (vite.config.ts pose la racine dans ui/).
export default defineConfig({ test: { include: ["test/**/*.test.ts"] } });
