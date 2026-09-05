export interface StoppableTrack {
  stop(): void;
}

export interface MediaStreamLike {
  getTracks(): readonly StoppableTrack[];
}

export interface PublicationLike {
  track?: StoppableTrack;
}

export interface LocalParticipantLike {
  trackPublications: ReadonlyMap<string, PublicationLike>;
  setMicrophoneEnabled(enabled: boolean): Promise<unknown>;
  setCameraEnabled(enabled: boolean): Promise<unknown>;
  unpublishTrack?(
    track: StoppableTrack,
    stopOnUnpublish?: boolean,
  ): unknown;
}

export interface RoomLike {
  localParticipant: LocalParticipantLike;
  disconnect(stopTracks?: boolean): unknown;
}

function stopTrackOnce(track: StoppableTrack, stopped: Set<StoppableTrack>): void {
  if (stopped.has(track)) return;
  stopped.add(track);
  track.stop();
}

/**
 * Synchronously stops every locally held media track before any network call.
 * This is the revocation boundary: callers may report revocation afterwards,
 * but capture does not wait for that receipt.
 */
export function stopLocalMediaImmediately(
  room: RoomLike | null,
  localStreams: readonly MediaStreamLike[] = [],
): number {
  const stopped = new Set<StoppableTrack>();
  for (const stream of localStreams) {
    for (const track of stream.getTracks()) {
      stopTrackOnce(track, stopped);
    }
  }
  if (room) {
    for (const publication of room.localParticipant.trackPublications.values()) {
      const track = publication.track;
      if (!track) continue;
      stopTrackOnce(track, stopped);
      room.localParticipant.unpublishTrack?.(track, true);
    }
  }
  return stopped.size;
}

export async function revokeLocalMedia(
  room: RoomLike | null,
  localStreams: readonly MediaStreamLike[] = [],
): Promise<number> {
  const stopped = stopLocalMediaImmediately(room, localStreams);
  if (room) {
    await Promise.allSettled([
      room.localParticipant.setMicrophoneEnabled(false),
      room.localParticipant.setCameraEnabled(false),
    ]);
  }
  return stopped;
}

export function leaveRoomImmediately(
  room: RoomLike | null,
  localStreams: readonly MediaStreamLike[] = [],
): number {
  const stopped = stopLocalMediaImmediately(room, localStreams);
  room?.disconnect(true);
  return stopped;
}
