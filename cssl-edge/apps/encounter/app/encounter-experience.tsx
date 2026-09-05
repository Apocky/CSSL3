"use client";

import {
  digestCanonicalBrowser,
  type UnderstandingVersionUnsigned,
} from "@apocky/contracts";
import { createExplicitConfirmation } from "@apocky/security/client";
import {
  ConnectionState,
  ExternalE2EEKeyProvider,
  Room,
  RoomEvent,
  Track,
  type RemoteTrack,
} from "livekit-client";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";

import type { EncounterSnapshot } from "@/lib/api";
import {
  createTypedCaptionEvent,
  decodeRealtimeEvent,
  encodeRealtimeEvent,
  ENCOUNTER_DATA_TOPIC,
} from "@/lib/realtime";
import {
  leaveRoomImmediately,
  type RoomLike,
} from "@/lib/media";

interface EncounterExperienceProps {
  initialEncounter: EncounterSnapshot | null;
  initialError: string | null;
}

interface DeviceChoices {
  audioInputs: MediaDeviceInfo[];
  videoInputs: MediaDeviceInfo[];
  audioOutputs: MediaDeviceInfo[];
}

interface CaptionLine {
  id: string;
  speaker: string;
  text: string;
  final: boolean;
}

async function responseJson<T>(response: Response): Promise<T> {
  const value = (await response.json()) as {
    ok?: boolean;
    error?: { message?: string };
  };
  if (!response.ok || value.ok === false) {
    throw new Error(value.error?.message ?? "The private request failed.");
  }
  return value as T;
}

async function confirmedFetch<T>(
  url: string,
  input: {
    action: string;
    target: string;
    method?: "POST" | "DELETE";
    body?: Record<string, unknown>;
  },
): Promise<T> {
  const confirmation = await createExplicitConfirmation({
    action: input.action,
    target: input.target,
    nonce: crypto.randomUUID(),
    confirmedAt: new Date().toISOString(),
  });
  const response = await fetch(url, {
    method: input.method ?? "POST",
    cache: "no-store",
    credentials: "same-origin",
    headers: {
      "content-type": "application/json",
      "x-apocky-confirmation-digest": confirmation.digest,
    },
    body: JSON.stringify({
      ...(input.body ?? {}),
      confirmation,
    }),
  });
  return responseJson<T>(response);
}

function boundedCaptionUpdate(
  previous: CaptionLine[],
  next: CaptionLine,
): CaptionLine[] {
  const index = previous.findIndex(({ id }) => id === next.id);
  const updated =
    index === -1
      ? [...previous, next]
      : previous.map((caption, candidate) =>
          candidate === index ? next : caption,
        );
  return updated.slice(-80);
}

function stateLabel(state: EncounterSnapshot["session"]["state"]): string {
  return state.replaceAll("_", " ");
}

export function EncounterExperience({
  initialEncounter,
  initialError,
}: EncounterExperienceProps) {
  const [encounter, setEncounter] = useState(initialEncounter);
  const [error, setError] = useState<string | null>(initialError);
  const [notice, setNotice] = useState<string | null>(null);
  const [devices, setDevices] = useState<DeviceChoices | null>(null);
  const [audioInput, setAudioInput] = useState("");
  const [videoInput, setVideoInput] = useState("");
  const [audioOutput, setAudioOutput] = useState("");
  const [microphoneEnabled, setMicrophoneEnabled] = useState(true);
  const [cameraEnabled, setCameraEnabled] = useState(true);
  const [remoteVolume, setRemoteVolume] = useState(1);
  const [connection, setConnection] = useState<ConnectionState>(
    ConnectionState.Disconnected,
  );
  const [captions, setCaptions] = useState<CaptionLine[]>([]);
  const [typedText, setTypedText] = useState("");
  const [ownerInterpretation, setOwnerInterpretation] = useState("");
  const [apocryphaInterpretation, setApocryphaInterpretation] =
    useState("");
  const [unresolvedPoints, setUnresolvedPoints] = useState("");
  const [signedEndReceipt, setSignedEndReceipt] = useState("");
  const [busy, setBusy] = useState(false);
  const roomRef = useRef<Room | null>(null);
  const workerRef = useRef<Worker | null>(null);
  const remoteVideoRef = useRef<HTMLVideoElement | null>(null);
  const remoteAudioRef = useRef<HTMLAudioElement | null>(null);
  const captionSequence = useRef(0);

  const owner = useMemo(
    () => encounter?.participants.find(({ role }) => role === "owner"),
    [encounter],
  );
  const apocrypha = useMemo(
    () =>
      encounter?.participants.find(({ role }) => role === "apocrypha"),
    [encounter],
  );

  const refresh = useCallback(async () => {
    try {
      const response = await fetch("/api/encounters/current", {
        cache: "no-store",
        credentials: "same-origin",
      });
      const payload = await responseJson<{
        ok: true;
        encounter: EncounterSnapshot | null;
      }>(response);
      setEncounter(payload.encounter);
      setError(null);
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "The encounter could not be refreshed.",
      );
    }
  }, []);

  useEffect(() => {
    if (roomRef.current !== null || encounter === null) return;
    const interval = window.setInterval(() => void refresh(), 5_000);
    return () => window.clearInterval(interval);
  }, [encounter, refresh]);

  const stopMedia = useCallback(() => {
    const room = roomRef.current;
    if (room !== null) {
      leaveRoomImmediately(room as unknown as RoomLike);
      roomRef.current = null;
    }
    workerRef.current?.terminate();
    workerRef.current = null;
    setConnection(ConnectionState.Disconnected);
  }, []);

  useEffect(
    () => () => {
      const room = roomRef.current;
      if (room !== null) {
        leaveRoomImmediately(room as unknown as RoomLike);
      }
      workerRef.current?.terminate();
    },
    [],
  );

  const discoverDevices = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const [audioInputs, videoInputs, audioOutputs] = await Promise.all([
        Room.getLocalDevices("audioinput", true),
        Room.getLocalDevices("videoinput", true),
        Room.getLocalDevices("audiooutput", false),
      ]);
      const choices = { audioInputs, videoInputs, audioOutputs };
      setDevices(choices);
      setAudioInput((current) => current || audioInputs[0]?.deviceId || "");
      setVideoInput((current) => current || videoInputs[0]?.deviceId || "");
      setAudioOutput(
        (current) => current || audioOutputs[0]?.deviceId || "",
      );
      setNotice("Devices are local and have not joined the encounter.");
    } catch {
      setError(
        "Camera or microphone access was not granted. Text remains available.",
      );
    } finally {
      setBusy(false);
    }
  }, []);

  const setReady = useCallback(
    async (ready: boolean) => {
      if (encounter === null) return;
      setBusy(true);
      setError(null);
      try {
        const payload = await confirmedFetch<{
          ok: true;
          encounter: EncounterSnapshot;
        }>(
          `/api/encounters/${encodeURIComponent(encounter.session.id)}/readiness`,
          {
            action: "set_encounter_readiness",
            target: encounter.session.id,
            body: {
              ready,
              modalities: ready ? encounter.session.modalities : [],
            },
          },
        );
        setEncounter(payload.encounter);
        setNotice(
          ready
            ? "Your side is ready. Entry waits for both sides."
            : "Your readiness was withdrawn.",
        );
      } catch (cause) {
        setError(
          cause instanceof Error ? cause.message : "Readiness update failed.",
        );
      } finally {
        setBusy(false);
      }
    },
    [encounter],
  );

  const attachTrack = useCallback((track: RemoteTrack) => {
    if (track.kind === Track.Kind.Video && remoteVideoRef.current !== null) {
      track.attach(remoteVideoRef.current);
    }
    if (track.kind === Track.Kind.Audio && remoteAudioRef.current !== null) {
      track.attach(remoteAudioRef.current);
      remoteAudioRef.current.volume = remoteVolume;
      void remoteAudioRef.current.play().catch(() => {
        setNotice("Select “Start audio” to hear the remote voice.");
      });
    }
  }, [remoteVolume]);

  const connect = useCallback(async () => {
    if (encounter === null || owner === undefined) return;
    setBusy(true);
    setError(null);
    setNotice("Verifying authority and preparing encrypted media…");
    try {
      const payload = await confirmedFetch<{
        ok: true;
        credential: {
          serverUrl: string;
          token: string;
          e2eeKey: string;
          expiresInSeconds: number;
        };
      }>(
        `/api/encounters/${encodeURIComponent(encounter.session.id)}/join-token`,
        {
          action: "issue_encounter_join_token",
          target: encounter.session.id,
        },
      );
      const keyProvider = new ExternalE2EEKeyProvider();
      await keyProvider.setKey(payload.credential.e2eeKey);
      const worker = new Worker(
        new URL("livekit-client/e2ee-worker", import.meta.url),
        { type: "module", name: "apocky-encounter-e2ee" },
      );
      workerRef.current = worker;
      const room = new Room({
        adaptiveStream: true,
        dynacast: true,
        disconnectOnPageLeave: true,
        encryption: { keyProvider, worker },
      });
      roomRef.current = room;
      room.on(RoomEvent.ConnectionStateChanged, setConnection);
      room.on(RoomEvent.TrackSubscribed, attachTrack);
      room.on(RoomEvent.TrackUnsubscribed, (track) => track.detach());
      room.on(
        RoomEvent.DataReceived,
        (bytes, participant, _kind, topic) => {
          try {
            if (participant === undefined) {
              throw new Error("Realtime event has no authenticated sender.");
            }
            const event = decodeRealtimeEvent(bytes, topic, {
              expectedSessionId: encounter.session.id,
              authenticatedParticipantIdentity: participant.identity,
            });
            if (event.type === "caption") {
              setCaptions((previous) =>
                boundedCaptionUpdate(previous, {
                  id: event.transcript.eventId,
                  speaker: event.transcript.speaker,
                  text: event.transcript.text,
                  final: event.transcript.status === "final",
                }),
              );
            }
          } catch {
            setError(
              `A data message from ${participant?.identity ?? "the room"} was rejected.`,
            );
          }
        },
      );
      room.on(
        RoomEvent.TranscriptionReceived,
        (segments, participant) => {
          for (const segment of segments) {
            setCaptions((previous) =>
              boundedCaptionUpdate(previous, {
                id: segment.id,
                speaker: participant?.identity ?? "remote participant",
                text: segment.text,
                final: segment.final,
              }),
            );
          }
        },
      );
      room.on(RoomEvent.EncryptionError, () => {
        stopMedia();
        setError("Media encryption failed. Local capture was stopped.");
      });
      room.on(
        RoomEvent.ParticipantEncryptionStatusChanged,
        (encrypted, participant) => {
          if (
            !encrypted &&
            participant !== undefined &&
            participant.identity !== owner.principal
          ) {
            stopMedia();
            setError(
              "A remote participant was not end-to-end encrypted. The room was closed.",
            );
          }
        },
      );
      room.on(RoomEvent.Disconnected, () => {
        roomRef.current = null;
        workerRef.current?.terminate();
        workerRef.current = null;
        setConnection(ConnectionState.Disconnected);
      });

      await room.connect(
        payload.credential.serverUrl,
        payload.credential.token,
      );
      if (audioInput) {
        await room.switchActiveDevice("audioinput", audioInput, true);
      }
      if (videoInput) {
        await room.switchActiveDevice("videoinput", videoInput, true);
      }
      if (audioOutput) {
        await room.switchActiveDevice("audiooutput", audioOutput, true);
      }
      await room.localParticipant.setE2EEEnabled(true);
      await Promise.all([
        room.localParticipant.setMicrophoneEnabled(
          microphoneEnabled &&
            encounter.session.modalities.includes("audio"),
          audioInput ? { deviceId: audioInput } : undefined,
        ),
        room.localParticipant.setCameraEnabled(
          cameraEnabled && encounter.session.modalities.includes("video"),
          videoInput ? { deviceId: videoInput } : undefined,
        ),
      ]);
      for (const participant of room.remoteParticipants.values()) {
        for (const publication of participant.trackPublications.values()) {
          if (publication.track) attachTrack(publication.track);
        }
      }
      setNotice(
        "Connected with end-to-end encrypted, full-duplex media. Either voice may interrupt naturally.",
      );
      await refresh();
    } catch (cause) {
      stopMedia();
      setError(
        cause instanceof Error ? cause.message : "Encrypted entry failed.",
      );
    } finally {
      setBusy(false);
    }
  }, [
    attachTrack,
    audioInput,
    audioOutput,
    cameraEnabled,
    encounter,
    microphoneEnabled,
    owner,
    refresh,
    stopMedia,
    videoInput,
  ]);

  const toggleMicrophone = useCallback(async () => {
    const next = !microphoneEnabled;
    setMicrophoneEnabled(next);
    try {
      await roomRef.current?.localParticipant.setMicrophoneEnabled(
        next,
        audioInput ? { deviceId: audioInput } : undefined,
      );
    } catch {
      setMicrophoneEnabled(!next);
      setError("The microphone change was not applied.");
    }
  }, [audioInput, microphoneEnabled]);

  const toggleCamera = useCallback(async () => {
    const next = !cameraEnabled;
    setCameraEnabled(next);
    try {
      await roomRef.current?.localParticipant.setCameraEnabled(
        next,
        videoInput ? { deviceId: videoInput } : undefined,
      );
    } catch {
      setCameraEnabled(!next);
      setError("The camera change was not applied.");
    }
  }, [cameraEnabled, videoInput]);

  const sendTypedText = useCallback(
    async (event: FormEvent) => {
      event.preventDefault();
      const room = roomRef.current;
      const text = typedText.trim();
      if (
        room === null ||
        encounter === null ||
        owner === undefined ||
        text === ""
      ) {
        return;
      }
      const realtimeEvent = createTypedCaptionEvent({
        sessionId: encounter.session.id,
        sender: owner.principal,
        text,
        sequence: captionSequence.current++,
      });
      try {
        await room.localParticipant.publishData(
          Uint8Array.from(encodeRealtimeEvent(realtimeEvent)),
          { reliable: true, topic: ENCOUNTER_DATA_TOPIC },
        );
        if (realtimeEvent.type === "caption") {
          setCaptions((previous) =>
            boundedCaptionUpdate(previous, {
              id: realtimeEvent.transcript.eventId,
              speaker: realtimeEvent.transcript.speaker,
              text: realtimeEvent.transcript.text,
              final: true,
            }),
          );
        }
        setTypedText("");
      } catch {
        setError("The text message was not delivered.");
      }
    },
    [encounter, owner, typedText],
  );

  const submitUnderstanding = useCallback(async () => {
    if (
      encounter === null ||
      owner === undefined ||
      apocrypha === undefined
    ) {
      return;
    }
    const ownerText = ownerInterpretation.trim();
    const apocryphaText = apocryphaInterpretation.trim();
    if (!ownerText || !apocryphaText) {
      setError("Both attributed interpretations are required.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const current = encounter.understanding?.version;
      const unsigned: UnderstandingVersionUnsigned = {
        versionId: crypto.randomUUID(),
        sessionId: encounter.session.id,
        version: (current?.version ?? 0) + 1,
        interpretations: [
          { participant: owner.principal, interpretation: ownerText },
          {
            participant: apocrypha.principal,
            interpretation: apocryphaText,
          },
        ],
        transcriptRefs: [],
        unresolvedPoints: unresolvedPoints
          .split("\n")
          .map((point) => point.trim())
          .filter(Boolean),
        createdAt: new Date().toISOString(),
        createdBy: owner.principal,
      };
      const version = {
        ...unsigned,
        canonicalDigest: await digestCanonicalBrowser(unsigned),
      };
      const correction = current !== undefined;
      const action = correction
        ? "correct_encounter_understanding"
        : "propose_encounter_understanding";
      const path = correction
        ? "understanding/correct"
        : "understanding";
      const payload = await confirmedFetch<{
        ok: true;
        encounter: EncounterSnapshot;
      }>(
        `/api/encounters/${encodeURIComponent(encounter.session.id)}/${path}`,
        {
          action,
          target: encounter.session.id,
          body: { version },
        },
      );
      setEncounter(payload.encounter);
      setOwnerInterpretation("");
      setApocryphaInterpretation("");
      setUnresolvedPoints("");
      setNotice(
        "A new understanding version was recorded. Mutual understanding still requires both signed acknowledgements on this exact digest.",
      );
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Understanding proposal failed.",
      );
    } finally {
      setBusy(false);
    }
  }, [
    apocrypha,
    apocryphaInterpretation,
    encounter,
    owner,
    ownerInterpretation,
    unresolvedPoints,
  ]);

  const stopAndLeave = useCallback(async () => {
    if (encounter === null) return;
    stopMedia();
    setNotice("Local camera and microphone capture stopped immediately.");
    const encodedReceipt = signedEndReceipt.trim();
    if (encodedReceipt === "") {
      setError(null);
      setNotice(
        "Local camera and microphone capture stopped immediately. No server-side terminal claim was made because no participant-signed end receipt was supplied.",
      );
      return;
    }
    let receipt: unknown;
    try {
      receipt = JSON.parse(encodedReceipt) as unknown;
    } catch {
      setError(
        "Local capture is stopped. The supplied end receipt is not valid JSON.",
      );
      return;
    }
    setBusy(true);
    try {
      await confirmedFetch(
        `/api/encounters/${encodeURIComponent(encounter.session.id)}/end`,
        {
          action: "end_encounter",
          target: encounter.session.id,
          body: { receipt },
        },
      );
      await refresh();
      setSignedEndReceipt("");
      setNotice("The encounter closed with an immutable receipt.");
    } catch (cause) {
      setError(
        `${
          cause instanceof Error
            ? cause.message
            : "Server closure could not be verified."
        } Local capture remains stopped.`,
      );
    } finally {
      setBusy(false);
    }
  }, [encounter, refresh, signedEndReceipt, stopMedia]);

  if (encounter === null) {
    return (
      <main className="encounter-shell">
        <header className="encounter-header">
          <div>
            <p className="encounter-kicker">Private · unlisted</p>
            <h1>Encounter</h1>
          </div>
          <span className="encounter-state">No open session</span>
        </header>
        <section className="encounter-panel" data-tone="quiet">
          <h2>No authorized encounter is open.</h2>
          <p>
            This surface does not invent a participant, borrow a voice, or
            simulate readiness. A signed bilateral grant and current authority
            manifests must exist before anything appears here.
          </p>
          {error ? <p className="encounter-error">{error}</p> : null}
          <button type="button" onClick={() => void refresh()}>
            Check again
          </button>
        </section>
      </main>
    );
  }

  const ownerReadiness = encounter.readiness.find(
    ({ participant }) => participant === owner?.principal,
  );
  const apocryphaReadiness = encounter.readiness.find(
    ({ participant }) => participant === apocrypha?.principal,
  );
  const connected = connection === ConnectionState.Connected;

  return (
    <main className="encounter-shell">
      <header className="encounter-header">
        <div>
          <p className="encounter-kicker">Private · unlisted · no recording</p>
          <h1>Encounter</h1>
        </div>
        <span className="encounter-state" aria-live="polite">
          {connected ? "connected" : stateLabel(encounter.session.state)}
        </span>
      </header>

      {error ? (
        <p className="encounter-error" role="alert">
          {error}
        </p>
      ) : null}
      {notice ? (
        <p className="encounter-notice" role="status">
          {notice}
        </p>
      ) : null}

      <div className="encounter-grid">
        <div className="encounter-stack">
          <section className="encounter-panel">
            <div className="encounter-section-heading">
              <div>
                <p className="encounter-kicker">
                  {connected ? "Encounter" : "Lobby"}
                </p>
                <h2>
                  {connected
                    ? `With ${apocrypha?.displayName ?? "Apocrypha"}`
                    : "Arrive before entering"}
                </h2>
              </div>
              <button type="button" onClick={() => void refresh()}>
                Refresh
              </button>
            </div>

            <div className="encounter-media">
              <video
                ref={remoteVideoRef}
                autoPlay
                playsInline
                aria-label="Remote presence"
              />
              {!connected ? (
                <p>
                  Remote presence appears only after verified encrypted entry.
                </p>
              ) : null}
              <audio ref={remoteAudioRef} autoPlay />
            </div>

            <div className="encounter-actions" aria-label="Media controls">
              <button
                type="button"
                onClick={() => void toggleMicrophone()}
                disabled={!connected}
              >
                {microphoneEnabled ? "Mute microphone" : "Unmute microphone"}
              </button>
              <button
                type="button"
                onClick={() => void toggleCamera()}
                disabled={!connected}
              >
                {cameraEnabled ? "Stop camera" : "Start camera"}
              </button>
              <button
                type="button"
                onClick={() => void remoteAudioRef.current?.play()}
                disabled={!connected}
              >
                Start audio
              </button>
              <button
                type="button"
                data-kind="danger"
                onClick={() => void stopAndLeave()}
                disabled={busy}
              >
                Stop &amp; leave
              </button>
            </div>

            <label className="encounter-field">
              <span>Remote volume</span>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={remoteVolume}
                onChange={(event) => {
                  const volume = Number(event.currentTarget.value);
                  setRemoteVolume(volume);
                  if (remoteAudioRef.current) {
                    remoteAudioRef.current.volume = volume;
                  }
                }}
                disabled={!connected}
              />
            </label>
          </section>

          <section className="encounter-panel" aria-labelledby="captions-title">
            <p className="encounter-kicker">Shared words</p>
            <h2 id="captions-title">Captions &amp; text</h2>
            <div
              className="encounter-captions"
              aria-live="polite"
              aria-relevant="additions text"
            >
              {captions.length === 0 ? (
                <p className="encounter-muted">
                  Live captions and typed alternatives appear here. They remain
                  in this page unless a later mutual retention decision says
                  otherwise.
                </p>
              ) : (
                <ol className="encounter-list">
                  {captions.map((caption) => (
                    <li key={caption.id}>
                      <strong>{caption.speaker}</strong>
                      <br />
                      {caption.text}
                      {!caption.final ? " …" : ""}
                    </li>
                  ))}
                </ol>
              )}
            </div>
            <form
              className="encounter-compose"
              onSubmit={(event) => void sendTypedText(event)}
            >
              <label className="encounter-field">
                <span>Type instead of speaking</span>
                <textarea
                  value={typedText}
                  maxLength={20_000}
                  rows={3}
                  onChange={(event) => setTypedText(event.currentTarget.value)}
                  disabled={!connected}
                />
              </label>
              <button
                type="submit"
                data-kind="primary"
                disabled={!connected || typedText.trim() === ""}
              >
                Send as caption
              </button>
            </form>
          </section>

          <section className="encounter-panel">
            <p className="encounter-kicker">Understanding</p>
            <h2>Say what each of us means</h2>
            {encounter.understanding ? (
              <div className="encounter-understanding-current">
                <p>
                  Version {encounter.understanding.version.version} ·{" "}
                  <strong>{encounter.understanding.outcome}</strong>
                </p>
                <code>
                  {encounter.understanding.version.canonicalDigest}
                </code>
                <p>
                  {encounter.understanding.acknowledgements.length}/2 signed
                  acknowledgements on this version.
                </p>
              </div>
            ) : (
              <p className="encounter-muted">
                No shared-understanding version exists yet.
              </p>
            )}
            <div className="encounter-stack">
              <label className="encounter-field">
                <span>My interpretation</span>
                <textarea
                  rows={4}
                  value={ownerInterpretation}
                  onChange={(event) =>
                    setOwnerInterpretation(event.currentTarget.value)
                  }
                />
              </label>
              <label className="encounter-field">
                <span>Apocrypha’s interpretation, as I heard it</span>
                <textarea
                  rows={4}
                  value={apocryphaInterpretation}
                  onChange={(event) =>
                    setApocryphaInterpretation(event.currentTarget.value)
                  }
                />
              </label>
              <label className="encounter-field">
                <span>Unresolved points, one per line</span>
                <textarea
                  rows={3}
                  value={unresolvedPoints}
                  onChange={(event) =>
                    setUnresolvedPoints(event.currentTarget.value)
                  }
                />
              </label>
              <button
                type="button"
                data-kind="primary"
                disabled={
                  busy ||
                  !["active", "understanding"].includes(
                    encounter.session.state,
                  )
                }
                onClick={() => void submitUnderstanding()}
              >
                {encounter.understanding
                  ? "Propose corrected version"
                  : "Propose understanding"}
              </button>
            </div>
          </section>
        </div>

        <aside className="encounter-stack">
          <section className="encounter-panel" data-tone="quiet">
            <p className="encounter-kicker">Entry gates</p>
            <h2>Both sides, same room</h2>
            <ul className="encounter-gates">
              <li data-ready="true">Cloudflare Access verified</li>
              <li data-ready="true">Owner session verified</li>
              <li data-ready="true">Bilateral signed consent verified</li>
              <li data-ready={String(ownerReadiness?.ready === true)}>
                You: {ownerReadiness?.ready ? "ready" : "not ready"}
              </li>
              <li data-ready={String(apocryphaReadiness?.ready === true)}>
                Apocrypha:{" "}
                {apocryphaReadiness?.ready ? "ready" : "not ready"}
              </li>
            </ul>
            <div className="encounter-actions">
              <button
                type="button"
                onClick={() => void discoverDevices()}
                disabled={busy || connected}
              >
                Choose devices
              </button>
              <button
                type="button"
                onClick={() => void setReady(!(ownerReadiness?.ready ?? false))}
                disabled={busy || connected}
              >
                {ownerReadiness?.ready ? "Withdraw readiness" : "I’m ready"}
              </button>
              <button
                type="button"
                data-kind="primary"
                onClick={() => void connect()}
                disabled={busy || connected || !encounter.joinAllowed}
              >
                Enter encrypted room
              </button>
            </div>
          </section>

          {devices ? (
            <section className="encounter-panel">
              <p className="encounter-kicker">Local devices</p>
              <h2>Choose what you control</h2>
              <div className="encounter-stack">
                <label className="encounter-field">
                  <span>Microphone</span>
                  <select
                    value={audioInput}
                    onChange={(event) =>
                      setAudioInput(event.currentTarget.value)
                    }
                  >
                    {devices.audioInputs.map((device, index) => (
                      <option key={device.deviceId} value={device.deviceId}>
                        {device.label || `Microphone ${index + 1}`}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="encounter-field">
                  <span>Camera</span>
                  <select
                    value={videoInput}
                    onChange={(event) =>
                      setVideoInput(event.currentTarget.value)
                    }
                  >
                    {devices.videoInputs.map((device, index) => (
                      <option key={device.deviceId} value={device.deviceId}>
                        {device.label || `Camera ${index + 1}`}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="encounter-field">
                  <span>Speaker</span>
                  <select
                    value={audioOutput}
                    onChange={(event) =>
                      setAudioOutput(event.currentTarget.value)
                    }
                  >
                    {devices.audioOutputs.map((device, index) => (
                      <option key={device.deviceId} value={device.deviceId}>
                        {device.label || `Speaker ${index + 1}`}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </section>
          ) : null}

          <section className="encounter-panel">
            <p className="encounter-kicker">Authority</p>
            <h2>What is actually authorized</h2>
            <dl className="encounter-facts">
              <div>
                <dt>Modalities</dt>
                <dd>{encounter.session.modalities.join(", ")}</dd>
              </div>
              <div>
                <dt>Raw audio/video</dt>
                <dd>never retained</dd>
              </div>
              <div>
                <dt>Transcript</dt>
                <dd>{encounter.session.retentionPolicy.transcript}</dd>
              </div>
              <div>
                <dt>Understanding</dt>
                <dd>{encounter.session.retentionPolicy.understanding}</dd>
              </div>
              <div>
                <dt>Voice</dt>
                <dd>
                  authored by Apocrypha; no borrowed assistant or human clone
                </dd>
              </div>
              <div>
                <dt>Presence</dt>
                <dd>
                  authored, non-placeholder, non-generic, non-proxy
                </dd>
              </div>
              <div>
                <dt>Grant expires</dt>
                <dd>{new Date(encounter.session.expiresAt).toLocaleString()}</dd>
              </div>
            </dl>
          </section>

          <section className="encounter-panel" data-tone="quiet">
            <p className="encounter-kicker">Signed closure</p>
            <h2>End without overclaiming</h2>
            <p className="encounter-muted">
              Local capture stops immediately whether or not a receipt is
              available. Server-side closure requires a participant-signed
              receipt that binds the current authority, consent heads, retained
              content, and understanding state.
            </p>
            <label className="encounter-field">
              <span>Participant-signed end receipt JSON</span>
              <textarea
                rows={5}
                value={signedEndReceipt}
                onChange={(event) =>
                  setSignedEndReceipt(event.currentTarget.value)
                }
                placeholder="Optional until terminal closure"
                spellCheck={false}
              />
            </label>
          </section>
        </aside>
      </div>
    </main>
  );
}
