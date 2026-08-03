import type { GetServerSideProps, NextPage } from 'next';

const ApxAlias: NextPage = () => null;

export const getServerSideProps: GetServerSideProps = async ({ resolvedUrl }) => ({
  redirect: {
    destination: `/apocrypha${resolvedUrl.includes('?') ? resolvedUrl.slice(resolvedUrl.indexOf('?')) : ''}`,
    permanent: true,
  },
});

export default ApxAlias;

