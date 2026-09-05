import type { GetServerSideProps, NextPage } from 'next';

const ChatAlias: NextPage = () => null;

export const getServerSideProps: GetServerSideProps = async ({ query, resolvedUrl }) => {
  const suffix = resolvedUrl.includes('?') ? resolvedUrl.slice(resolvedUrl.indexOf('?')) : '';
  const target = `/apocrypha${suffix}`;
  void query;
  return { redirect: { destination: target, permanent: true } };
};

export default ChatAlias;

