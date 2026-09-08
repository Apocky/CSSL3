import Head from 'next/head';
import { useRouter } from 'next/router';
import { useState } from 'react';

import { ClearingRoom, type ClearingContextAxis } from '../components/clearing/ClearingRoom';
import { useClearingData } from '../components/clearing/useClearingData';
import { useSiteSession } from '../components/hub/SiteSession';

const CLEARING_PATH = '/clearing';

export default function ClearingPage(): JSX.Element {
  const router = useRouter();
  const { access } = useSiteSession();
  const requestedRoom = typeof router.query.room === 'string' ? router.query.room : 'north-clearing';
  const data = useClearingData(requestedRoom);
  const [draft, setDraft] = useState('');
  const [selectedMessageId, setSelectedMessageId] = useState<string | null>(null);
  const [contextAxis, setContextAxis] = useState<ClearingContextAxis | null>(null);
  const [sending, setSending] = useState(false);
  const [surfaceNotice, setSurfaceNotice] = useState<string | null>(null);

  const selectRoom = (slug: string): void => {
    setSelectedMessageId(null);
    setContextAxis(null);
    setSurfaceNotice(null);
    void router.replace({ pathname: CLEARING_PATH, query: { room: slug } }, undefined, { shallow: true });
  };

  const send = async (body: string, replyTo: string | null): Promise<void> => {
    setSurfaceNotice(null);
    setSending(true);
    const result = await data.sendMessage(body, replyTo);
    setSending(false);
    if (result.ok) {
      setDraft('');
    } else {
      setSurfaceNotice(result.error ?? 'The room could not place that message.');
    }
  };

  return (
    <>
      <Head>
        <title>{`${data.activeRoom?.title ?? 'The Clearing'} · Apocky`}</title>
        <meta name="description" content="A live public social room for shared messages, threads, and small discoveries." />
        <meta property="og:title" content="The Clearing · Apocky" />
        <link rel="canonical" href="https://www.apocky.com/clearing" />
      </Head>
      <ClearingRoom
        rooms={data.rooms}
        activeRoomId={data.activeRoom?.id ?? ''}
        messages={data.messages}
        reactions={data.reactions}
        members={data.members}
        selectedMessageId={selectedMessageId}
        contextAxis={contextAxis}
        draft={draft}
        liveState={data.liveState}
        session={access === 'checking' ? 'checking' : access === 'signed-out' || access === 'unavailable' ? 'signed-out' : 'signed-in'}
        presenceCount={data.presenceCount}
        error={surfaceNotice || data.error}
        sending={sending}
        onSelectRoom={selectRoom}
        onSelectMessage={setSelectedMessageId}
        onOpenContext={(id) => { setSelectedMessageId(id); setContextAxis(null); }}
        onCloseContext={() => { setSelectedMessageId(null); setContextAxis(null); }}
        onSetContextAxis={setContextAxis}
        onDraftChange={(value) => { setDraft(value); if (surfaceNotice) setSurfaceNotice(null); }}
        onSend={send}
        onReact={(messageId, kind) => {
          if (access !== 'member' && access !== 'owner') {
            void router.push('/login?next=%2Fclearing');
            return;
          }
          void data.toggleReaction(messageId, kind).then((result) => {
            if (!result.ok) setSurfaceNotice(result.error ?? 'The reaction could not be placed.');
          });
        }}
        onReply={(id) => { setSelectedMessageId(id); setContextAxis('Meaning'); }}
        onInvite={() => {
          const roomUrl = window.location.href;
          if (navigator.share) {
            void navigator.share({ title: data.activeRoom?.title ?? 'The Clearing', url: roomUrl })
              .then(() => setSurfaceNotice('Room invitation ready.'))
              .catch((error: unknown) => {
                if (error instanceof DOMException && error.name === 'AbortError') return;
                setSurfaceNotice('Share was unavailable. Copy the address from your browser.');
              });
            return;
          }
          if (!navigator.clipboard) {
            setSurfaceNotice('Copy the room address from your browser.');
            return;
          }
          void navigator.clipboard.writeText(roomUrl)
            .then(() => setSurfaceNotice('Room link copied.'))
            .catch(() => setSurfaceNotice('Copy the room address from your browser.'));
        }}
        onPrivacy={() => { void router.push('/legal/privacy'); }}
        onSignIn={() => { void router.push('/login?next=%2Fclearing'); }}
      />
    </>
  );
}
