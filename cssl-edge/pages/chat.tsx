import type { GetServerSideProps, NextPage } from 'next';

const LegacyChatRedirect: NextPage = () => null;

export const getServerSideProps: GetServerSideProps = async () => ({
  redirect: {
    destination: '/admin/chat',
    permanent: false,
  },
});

export default LegacyChatRedirect;