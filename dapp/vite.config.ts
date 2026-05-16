import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  return {
    plugins: [react()],
    base: env.VITE_BASE_PATH || "/dapp/",
    server: {
      host: "127.0.0.1",
      port: 5174,
      proxy: {
        "/v1": env.VITE_API_BASE_URL || "http:///66.245.218.165:8787"
      }
    }
  };
});
