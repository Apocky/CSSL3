// /admin/apocrypha → /admin/chat (per Apocky : "Apocrypha is the name, not a nav element").
// Server-side 308 redirect via next.config.js handles the canonical path ; this client
// fallback covers anyone landing here through stale Next.js client-side router state.

import type { NextPage, GetServerSideProps } from 'next';

export const getServerSideProps: GetServerSideProps = async () => ({
  redirect: { destination: '/admin/chat', permanent: true },
});

const Redirect: NextPage = () => null;
export default Redirect;
