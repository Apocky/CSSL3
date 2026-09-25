/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // The desktop direct server builds into its own folder so a dev build never pulls the rug from it.
  distDir: process.env.NEXT_DIST_DIR || '.next',
  // Playwright binds the local verification server to this loopback host.
  // Declare that development origin explicitly so Next does not treat its
  // own test assets as a future cross-origin error. Production is unaffected.
  allowedDevOrigins: ['127.0.0.1'],
  // Restrict route discovery to .ts/.tsx (skip *.test.ts files even if accidentally
  // dropped under pages/). Tests live in tests/ outside pages/ by convention.
  pageExtensions: ['ts', 'tsx', 'js', 'jsx'],
  // commit-sha exposed to runtime via env (Vercel auto-injects VERCEL_GIT_COMMIT_SHA)
  env: {
    CSSL_EDGE_VERSION: '0.1.0',
  },
  // Approved bodies are intentionally absent today. When an owner-reviewed body is
  // added later, trace only that approved store into the dynamic API function.
  outputFileTracingIncludes: {
    '/api/conversation-corpus/[id]': ['./public/conversation-corpus/approved-records/**/*.json'],
  },
  async rewrites() {
    return {
      beforeFiles: [
        { source: '/codex-apockalypsis', destination: '/codex-apockalypsis/index.html' },
        { source: '/codex-apockalypsis/library/:slug', destination: '/codex-apockalypsis/library/:slug/index.html' },
      ],
    };
  },
  async redirects() {
    return [
      // The bubble chat is retired (2026-09-24): the living room is the only Apocrypha surface.
      { source: '/apocrypha', destination: '/room', permanent: true },
      { source: '/chat', destination: '/room', permanent: true },
      // Binary Tarot now belongs to the Chaos Tarot divination platform.
      { source: '/oracle', destination: 'https://chaos-tarot.com/yes-no?source=apocky-oracle', permanent: true },
      // The old Commons hub is superseded by the native React homepage.
      { source: '/commons', destination: '/', permanent: true },
      { source: '/commons/index.html', destination: '/', permanent: true },
      { source: '/commons/clearing.html', destination: '/clearing', permanent: true },
      { source: '/commons/atlas.html', destination: '/atlas', permanent: true },
      { source: '/commons/membership.html', destination: '/membership', permanent: true },
      { source: '/commons/principles.html', destination: '/principles', permanent: true },
    ];
  },
};

module.exports = nextConfig;
