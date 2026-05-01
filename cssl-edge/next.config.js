/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // Restrict route discovery to .ts/.tsx (skip *.test.ts files even if accidentally
  // dropped under pages/). Tests live in tests/ outside pages/ by convention.
  pageExtensions: ['ts', 'tsx', 'js', 'jsx'],
  // commit-sha exposed to runtime via env (Vercel auto-injects VERCEL_GIT_COMMIT_SHA)
  env: {
    CSSL_EDGE_VERSION: '0.1.0',
  },
};

module.exports = nextConfig;
