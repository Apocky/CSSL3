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
  // Permanent redirects for the legacy /admin/apocrypha/* sub-routes.
  // Per Apocky's nav-clarification : "Apocrypha is the name, not a nav element".
  // Sub-routes moved to top-level /admin/{chat,diagnostics,controls,sub-minds}.
  async redirects() {
    return [
      { source: '/admin/apocrypha/chat', destination: '/admin/chat', permanent: true },
      { source: '/admin/apocrypha/diag', destination: '/admin/diagnostics', permanent: true },
      { source: '/admin/apocrypha/controls', destination: '/admin/controls', permanent: true },
      { source: '/admin/apocrypha/cockpit', destination: '/admin/diagnostics', permanent: true },
      // /admin/tasks was LoA scheduling content ; Apocrypha-equivalent = /admin/sub-minds
      { source: '/admin/tasks', destination: '/admin/sub-minds', permanent: true },
    ];
  },
};

module.exports = nextConfig;
