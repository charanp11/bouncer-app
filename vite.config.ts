import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: {
    // Localhost only: the dev server must never be reachable from the network.
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/crates/**"] },
  },
});
