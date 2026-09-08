import React, { useCallback, useEffect, useRef, useState } from 'react';

import { authFetch } from '../../lib/browser-auth';

type VisionStatus = 'idle' | 'starting' | 'live' | 'paused' | 'stopping' | 'unavailable';

function metadataOnly(value: unknown): boolean {
  if (!value || typeof value !== 'object') return true;
  if (Array.isArray(value)) return value.every(metadataOnly);
  for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
    if (['content_b64', 'frame_bytes', 'raw_frame_bytes', 'image_data', 'data_uri'].includes(key)) return false;
    if (!metadataOnly(child)) return false;
  }
  return true;
}

function randomUuid(): string {
  return crypto.randomUUID().toLowerCase();
}

async function blobBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('camera frame encoding failed'));
    reader.onload = () => {
      const result = typeof reader.result === 'string' ? reader.result : '';
      const comma = result.indexOf(',');
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.readAsDataURL(blob);
  });
}

export function VisionPanel() {
  const [status, setStatus] = useState<VisionStatus>('idle');
  const [sessionRef, setSessionRef] = useState<string | null>(null);
  const [projection, setProjection] = useState<Record<string, unknown> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const frameTimerRef = useRef<number | null>(null);
  const sequenceRef = useRef(0);
  const sessionRefValue = useRef<string | null>(null);

  const stopCamera = useCallback(async (event: 'close' | 'pause' = 'close') => {
    const currentSession = sessionRefValue.current;
    if (frameTimerRef.current !== null) {
      window.clearInterval(frameTimerRef.current);
      frameTimerRef.current = null;
    }
    setStatus('stopping');
    if (currentSession) {
      try {
        await authFetch(`/api/admin/apocrypha/vision/session/${currentSession}/control`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ event }),
          cache: 'no-store',
        });
      } catch {
        // Local tracks are still stopped if the control receipt is unavailable.
      }
    }
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    if (videoRef.current) videoRef.current.srcObject = null;
    sessionRefValue.current = null;
    setSessionRef(null);
    setStatus('idle');
  }, []);

  const captureFrame = useCallback(async () => {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    const currentSession = sessionRefValue.current;
    if (!video || !canvas || !currentSession || video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) return;
    const width = Math.min(video.videoWidth || 640, 640);
    const height = Math.min(video.videoHeight || 480, 480);
    if (!width || !height) return;
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d', { alpha: false });
    if (!context) return;
    context.drawImage(video, 0, 0, width, height);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.72));
    if (!blob || !sessionRefValue.current) return;
    const encoded = await blobBase64(blob);
    const response = await authFetch(`/api/admin/apocrypha/vision/session/${currentSession}/frame`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      cache: 'no-store',
      body: JSON.stringify({
        sequence: sequenceRef.current,
        captured_at_unix_ns: Date.now() * 1_000_000,
        recorded_at_unix_ns: Date.now() * 1_000_000,
        media_type: 'image/jpeg',
        content_b64: encoded,
        input_mirrored: false,
        clockwise_rotation_degrees: 0,
      }),
    });
    sequenceRef.current += 1;
    if (response.ok) {
      const body = await response.json() as { data?: unknown };
      const nextProjection = body.data && typeof body.data === 'object' ? body.data as Record<string, unknown> : null;
      if (nextProjection && metadataOnly(nextProjection)) setProjection(nextProjection);
    }
  }, []);

  const startCamera = useCallback(async () => {
    if (!navigator.mediaDevices?.getUserMedia) {
      setError('This browser does not expose camera permission controls.');
      setStatus('unavailable');
      return;
    }
    setError(null);
    setStatus('starting');
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
      const nextSession = randomUuid();
      const consentId = randomUuid();
      const response = await authFetch('/api/admin/apocrypha/vision/session', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        body: JSON.stringify({ session_ref: nextSession, consent_id: consentId, purpose: 'webcam_perception', duration_seconds: 300, max_fps: 5 }),
      });
      if (!response.ok) throw new Error('Apocrypha did not grant a vision session.');
      streamRef.current = stream;
      sessionRefValue.current = nextSession;
      setSessionRef(nextSession);
      if (videoRef.current) {
        videoRef.current.srcObject = stream;
        await videoRef.current.play();
      }
      sequenceRef.current = 0;
      setStatus('live');
    } catch (cause) {
      streamRef.current?.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
      setStatus('unavailable');
      setError(cause instanceof Error ? cause.message : 'Camera consent or vision session failed.');
    }
  }, []);

  useEffect(() => {
    if (status !== 'live') return;
    frameTimerRef.current = window.setInterval(() => {
      void captureFrame();
    }, 200) as unknown as number;
    return () => {
      if (frameTimerRef.current !== null) window.clearInterval(frameTimerRef.current);
    };
  }, [captureFrame, status]);

  useEffect(() => () => { void stopCamera(); }, [stopCamera]);

  return (
    <section className="v2-vision" aria-label="Apocrypha vision input">
      <header>
        <div>
          <p className="eyebrow">VISION INPUT</p>
          <h2>Camera perception</h2>
        </div>
        {status === 'idle' || status === 'unavailable' ? (
          <button type="button" onClick={() => void startCamera()}>Start with camera consent</button>
        ) : (
          <button type="button" onClick={() => void stopCamera()} disabled={status === 'stopping'}>
            {status === 'stopping' ? 'Stopping…' : 'Stop camera'}
          </button>
        )}
      </header>
      <p className="notice">No camera opens until you press start. Raw frames are held only long enough for one bounded projection request; they are not persisted or displayed as telemetry.</p>
      <video ref={videoRef} muted playsInline aria-label="Local camera preview" />
      <canvas ref={canvasRef} hidden />
      <div className="meta" aria-live="polite">
        <span>Status · {status}</span>
        <span>Session · {sessionRef ? sessionRef.slice(0, 8) : 'none'}</span>
        <span>Projection · {projection ? 'metadata received' : 'none'}</span>
      </div>
      {error && <p className="error" role="alert">{error}</p>}
      <style jsx>{`
        .v2-vision { display:grid; gap:10px; padding:14px 16px; border:1px solid #29263b; border-radius:14px; color:#dfe1ec; background:#0e0e16; }
        header { display:flex; align-items:center; justify-content:space-between; gap:12px; }
        .eyebrow { margin:0 0 4px; color:#9e8cff; font:700 .64rem/1 ui-monospace,monospace; letter-spacing:.16em; }
        h2 { margin:0; font-size:1rem; }
        button { min-height:40px; padding:0 12px; border:1px solid #403b58; border-radius:999px; color:#f1effb; background:#181522; cursor:pointer; }
        button:focus-visible { outline:2px solid #b9a8ff; outline-offset:3px; }
        .notice { margin:0; color:#9b99aa; font-size:.74rem; line-height:1.55; }
        video { width:100%; max-height:260px; border-radius:10px; background:#06060a; object-fit:contain; }
        .meta { display:flex; flex-wrap:wrap; gap:6px 14px; color:#9693a8; font:600 .68rem/1.4 ui-monospace,monospace; }
        .error { margin:0; color:#ffb8bd; font-size:.78rem; }
      `}</style>
    </section>
  );
}
