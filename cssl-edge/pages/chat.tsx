import type { GetServerSideProps } from 'next';

export const getServerSideProps: GetServerSideProps = async ({ resolvedUrl }) => {
  const queryIndex = resolvedUrl.indexOf('?');
  const suffix = queryIndex >= 0 ? resolvedUrl.slice(queryIndex) : '';
  return {
    redirect: {
      destination: `/apocrypha${suffix}`,
      permanent: true,
    },
  };
};

export default function ChatAlias(): null {
  return null;
}
