import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

export default defineConfig({
  plugins: [react()],
  define: {
    global: "globalThis",
  },
  server: {
    port: 5173,
    fs: {
      allow: [path.resolve(__dirname, ".."), __dirname],
    },
  },
});
