import type { GetServerSideProps, NextPage } from 'next';

// Apocrypha IS the chat. The old standalone showcase is gone; this redirects to the real thing.
const ApocryphaRedirect: NextPage = () => null;

export const getServerSideProps: GetServerSideProps = async () => ({
  redirect: { destination: '/chat', permanent: false },
});

export default ApocryphaRedirect;
