import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Interface du plugin, servie par iBox sous http://iboxplugin.localhost/iclips/ui/ :
// adresses relatives, trois pages : le Studio, le panneau en jeu et la notification (app autonome).
export default defineConfig({
  root: "ui",
  base: "./",
  plugins: [react(), tailwindcss()],
  build: {
    outDir: "../dist/ui",
    emptyOutDir: true,
    rollupOptions: { input: ["ui/studio.html", "ui/panel.html", "ui/toast.html"] },
  },
});
