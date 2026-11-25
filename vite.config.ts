import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Interface du plugin, servie par iBox sous http://iboxplugin.localhost/iclips/ui/ :
// adresses relatives, deux pages : le Studio et le panneau en jeu.
export default defineConfig({
  root: "ui",
  base: "./",
  plugins: [react(), tailwindcss()],
  build: {
    outDir: "../dist/ui",
    emptyOutDir: true,
    rollupOptions: { input: ["ui/studio.html", "ui/panel.html"] },
  },
});
