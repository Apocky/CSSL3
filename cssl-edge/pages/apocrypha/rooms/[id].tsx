import Head from 'next/head';
import { useRouter } from 'next/router';
import { useEffect, useState } from 'react';

import { ApocryphaThreshold, resolveThresholdState } from '../../../components/apocrypha/Threshold';
import { usePublicPresence } from '../../../components/apocrypha/usePublicPresence';
import { ClearingRoom, type ClearingContextAxis } from '../../../components/clearing/ClearingRoom';
import { useClearingData } from '../../../components/clearing/useClearingData';
import { useSiteSession } from '../../../components/hub/SiteSession';
import type { ClearingReaction } from '../../../lib/clearing/client';

const DEFAULT_ROOM = 'north-clearing';
const ROOM_SLUG = /^[a-z0-9][a-z0-9-]{1,47}$/;

function OwnerClearingRoom({ roomSlug }: { roomSlug: string }): JSX.Element {
  const router = useRouter();
  const data = useClearingData(roomSlug);
  const [draft, setDraft] = useState('');
  const [selectedMessageId, setSelectedMessageId] = useState<string | null>(null);
  const [contextAxis, setContextAxis] = useState<ClearingContextAxis | null>(null);
  const [sending, setSending] = useState(false);
  const [surfaceNotice, setSurfaceNotice] = useState('');

  useEffect(() => {
    setSelectedMessageId(null);
    setContextAxis(null);
    setSurfaceNotice('');
  }, [roomSlug]);

  const selectRoom = (slug: string): void => {
    void router.push(`/apocrypha/rooms/${encodeURIComponent(slug)}`);
  };

  const send = async (body: string, replyTo: string | null): Promise<boolean> => {
    setSurfaceNotice('');
    setSending(true);
    const result = await data.sendMessage(body, replyTo);
    setSending(false);
    if (result.ok) {
      setDraft('');
      return true;
    }
    setSurfaceNotice(result.error ?? 'The room could not place that message.');
    return false;
  };

  const react = async (
    messageId: string,
    kind: ClearingReaction['kind'],
  ): Promise<void> => {
    setSurfaceNotice('');
    const result = await data.toggleReaction(messageId, kind);
    if (!result.ok) setSurfaceNotice(result.error ?? 'The reaction was not changed.');
  };

  const withdraw = async (messageId: string): Promise<boolean> => {
    setSurfaceNotice('');
    const result = await data.withdrawMessage(messageId);
    if (!result.ok) {
      setSurfaceNotice(result.error ?? 'The message was not withdrawn.');
      return false;
    }
    if (selectedMessageId === messageId) {
      setSelectedMessageId(null);
      setContextAxis(null);
    }
    setSurfaceNotice('Your message was withdrawn from public view.');
    return true;
  };

  const invite = (): void => {
    void (async () => {
      try {
        await navigator.clipboard.writeText(window.location.href);
        setSurfaceNotice('Room link copied.');
      } catch {
        setSurfaceNotice('Copy failed. Use the address in your browser.');
      }
    })();
  };

  return (
    <>
      <Head>
        <title>{data.activeRoom?.title ?? 'The Clearing'} · Apocky</title>
        <meta
          name="description"
          content="A beta room for public-record messages, replies, and reactions."
        />
        <meta property="og:title" content={`${data.activeRoom?.title ?? 'The Clearing'} · Apocky`} />
        <link
          rel="canonical"
          href={`https://www.apocky.com/apocrypha/rooms/${encodeURIComponent(roomSlug)}`}
        />
      </Head>
      <ClearingRoom
        rooms={data.rooms}
        activeRoomId={data.activeRoom?.id ?? ''}
        messages={data.messages}
        reactions={data.reactions}
        selectedMessageId={selectedMessageId}
        contextAxis={contextAxis}
        draft={draft}
        liveState={data.liveState}
        session={data.authState}
        currentActorRef={data.actorRef}
        error={surfaceNotice || data.error}
        sending={sending}
        onSelectRoom={selectRoom}
        onSelectMessage={setSelectedMessageId}
        onOpenContext={(id) => {
          setSelectedMessageId(id);
          setContextAxis((current) => current ?? 'People');
        }}
        onCloseContext={() => {
          setSelectedMessageId(null);
          setContextAxis(null);
        }}
        onSetContextAxis={setContextAxis}
        onDraftChange={(value) => {
          setDraft(value);
          if (surfaceNotice) setSurfaceNotice('');
        }}
        onSend={send}
        onReact={react}
        onReply={(id) => {
          setSelectedMessageId(id);
          setContextAxis('Meaning');
        }}
        onWithdraw={withdraw}
        onInvite={invite}
        onExit={() => { void router.push(`/apocrypha?room=${encodeURIComponent(roomSlug)}`); }}
        onRetry={() => { void data.reload(); }}
        onSignIn={() => {
          void router.push(`/login?next=${encodeURIComponent(`/apocrypha/rooms/${roomSlug}`)}`);
        }}
      />
    </>
  );
}

export default function ClearingRoomRoute(): JSX.Element {
  const router = useRouter();
  const session = useSiteSession();
  const presence = usePublicPresence();
  const rawRoom = typeof router.query.id === 'string' ? router.query.id.toLowerCase() : DEFAULT_ROOM;
  const room = ROOM_SLUG.test(rawRoom) ? rawRoom : null;
  const access = router.isReady && room ? session.access : 'checking';
  const thresholdState = resolveThresholdState(access, presence.state);

  if (thresholdState === 'owner' && room) {
    return <OwnerClearingRoom roomSlug={room} />;
  }

  const retry = (): void => {
    void Promise.all([session.refresh(), presence.refresh()]);
  };
  const returnPath = room
    ? `/apocrypha/rooms/${encodeURIComponent(room)}`
    : '/apocrypha';

  return (
    <>
      <Head>
        <title>Apocrypha · private threshold</title>
        <meta name="description" content="A fail-closed doorway to The Clearing." />
        <link rel="canonical" href="https://www.apocky.com/apocrypha" />
      </Head>
      <ApocryphaThreshold
        access={room ? access : 'unavailable'}
        presence={presence.state}
        roomHref={returnPath}
        signInHref={`/login?next=${encodeURIComponent(returnPath)}`}
        onRetry={retry}
      />
    </>
  );
}
