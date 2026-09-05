import assert from "node:assert/strict";
import test from "node:test";

import {
  leaveRoomImmediately,
  revokeLocalMedia,
  stopLocalMediaImmediately,
  type RoomLike,
  type StoppableTrack,
} from "../lib/media";

function fixture(): {
  room: RoomLike;
  localTrack: StoppableTrack;
  previewTrack: StoppableTrack;
  events: string[];
} {
  const events: string[] = [];
  const localTrack = { stop: () => events.push("stop:local") };
  const previewTrack = { stop: () => events.push("stop:preview") };
  const room: RoomLike = {
    localParticipant: {
      trackPublications: new Map([["camera", { track: localTrack }]]),
      setMicrophoneEnabled: async () => {
        events.push("network:microphone");
      },
      setCameraEnabled: async () => {
        events.push("network:camera");
      },
      unpublishTrack: () => {
        events.push("unpublish");
      },
    },
    disconnect: () => {
      events.push("disconnect");
    },
  };
  return { room, localTrack, previewTrack, events };
}

test("revocation stops local capture before asynchronous room mutations", async () => {
  const { room, previewTrack, events } = fixture();
  const pending = revokeLocalMedia(room, [
    { getTracks: () => [previewTrack] },
  ]);
  assert.deepEqual(events.slice(0, 3), [
    "stop:preview",
    "stop:local",
    "unpublish",
  ]);
  await pending;
  const firstNetwork = events.findIndex((event) => event.startsWith("network:"));
  assert.ok(firstNetwork >= 3);
});

test("the same physical track is stopped once even when held twice", () => {
  const { room, localTrack, events } = fixture();
  const count = stopLocalMediaImmediately(room, [
    { getTracks: () => [localTrack] },
  ]);
  assert.equal(count, 1);
  assert.equal(events.filter((event) => event === "stop:local").length, 1);
});

test("leaving stops tracks before disconnecting", () => {
  const { room, events } = fixture();
  leaveRoomImmediately(room);
  assert.ok(events.indexOf("stop:local") < events.indexOf("disconnect"));
});
