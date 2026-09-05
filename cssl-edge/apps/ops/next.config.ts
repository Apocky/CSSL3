import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  poweredByHeader: false,
  reactStrictMode: true,
  transpilePackages: [
    "@apocky/contracts",
    "@apocky/security",
    "@apocky/visual-tokens",
  ],
};

export default nextConfig;
