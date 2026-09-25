// Desktop direct mode only (viewer.direct): voice in, voice out, notifications, and the PC's
// service control panel. Every call goes to /api/direct/*, which is 404 anywhere but the owner's PC.
import { useEffect, useRef, useState } from 'react';

import styles from './DirectTools.module.css';
import type { RoomEventView } from './types';

interface Service { key: string; name: string; what: string; port: number; up: boolean; restartable: boolean }

const NOTIFY_KEY = 'apocrypha.direct.notify';

function spoken(body: string): string {
  return body.replace(/<think>[\s\S]*?<\/think>/gu, '').replace(/[*_`#>]/gu, '').trim();
}

export default function DirectTools({ events, onText }: { readonly events: readonly RoomEventView[]; readonly onText: (text: string) => void }): JSX.Element {
  const [recording, setRecording] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [notify, setNotify] = useState(false);
  const [panel, setPanel] = useState(false);
  const [services, setServices] = useState<Service[]>([]);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const recorder = useRef<MediaRecorder | null>(null);
  const audio = useRef<HTMLAudioElement | null>(null);
  const lastSeen = useRef<number>(0);

  const replies = events.filter((e) => e.author === 'apocrypha' && e.kind === 'utterance');
  const last = replies.at(-1);

  useEffect(() => { try { setNotify(localStorage.getItem(NOTIFY_KEY) === '1'); } catch { /* storage blocked */ } }, []);

  // Notify on a new reply while the window is hidden.
  useEffect(() => {
    if (!last) return;
    if (lastSeen.current === 0) { lastSeen.current = last.id; return; }
    if (last.id <= lastSeen.current) return;
    lastSeen.current = last.id;
    if (notify && document.hidden && typeof Notification !== 'undefined' && Notification.permission === 'granted') {
      new Notification('Apocrypha', { body: spoken(last.body).slice(0, 180) });
    }
  }, [last, notify]);

  async function toggleNotify(): Promise<void> {
    const next = !notify;
    if (next && typeof Notification !== 'undefined' && Notification.permission !== 'granted') {
      if (await Notification.requestPermission() !== 'granted') { setNote('Notifications were not allowed.'); return; }
    }
    setNotify(next);
    try { localStorage.setItem(NOTIFY_KEY, next ? '1' : '0'); } catch { /* storage blocked */ }
  }

  async function toggleMic(): Promise<void> {
    if (recording) { recorder.current?.stop(); return; }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const rec = new MediaRecorder(stream);
      const chunks: Blob[] = [];
      rec.ondataavailable = (e) => { if (e.data.size) chunks.push(e.data); };
      rec.onstop = async () => {
        stream.getTracks().forEach((t) => t.stop());
        setRecording(false);
        setNote('Transcribing…');
        const blob = new Blob(chunks, { type: rec.mimeType || 'audio/webm' });
        const res = await fetch('/api/direct/stt', { method: 'POST', headers: { 'content-type': blob.type }, body: blob });
        const payload = await res.json().catch(() => ({})) as { text?: string; error?: string };
        if (res.ok && payload.text) { onText(payload.text); setNote(null); } else setNote(payload.error ?? 'Nothing heard.');
      };
      recorder.current = rec;
      rec.start();
      setRecording(true);
      setNote('Listening… click again to stop.');
    } catch { setNote('The microphone is not available.'); }
  }

  async function readAloud(): Promise<void> {
    if (!last) return;
    setNote('Speaking…');
    const res = await fetch('/api/direct/tts', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ text: spoken(last.body) }) });
    if (!res.ok) { setNote('Could not speak that.'); return; }
    const url = URL.createObjectURL(await res.blob());
    audio.current?.pause();
    audio.current = new Audio(url);
    audio.current.onended = () => { URL.revokeObjectURL(url); setNote(null); };
    void audio.current.play();
  }

  async function loadServices(): Promise<void> {
    const res = await fetch('/api/direct/services');
    const payload = await res.json().catch(() => ({})) as { services?: Service[] };
    setServices(payload.services ?? []);
  }

  async function restart(key: string): Promise<void> {
    setBusyKey(key);
    const res = await fetch('/api/direct/services', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ key }) });
    const payload = await res.json().catch(() => ({})) as { detail?: string };
    setNote(payload.detail ?? (res.ok ? 'Restarted.' : 'Restart failed.'));
    setBusyKey(null);
    void loadServices();
  }

  return <div className={styles.bar}>
    <button type="button" className={recording ? styles.on : styles.btn} onClick={() => void toggleMic()} title="Speak a message (transcribed on your PC)">{recording ? '● Stop' : '🎙 Speak'}</button>
    <button type="button" className={styles.btn} onClick={() => void readAloud()} disabled={!last} title="Read Apocrypha's last reply aloud">🔊 Read aloud</button>
    <button type="button" className={notify ? styles.on : styles.btn} onClick={() => void toggleNotify()} title="Notify me of replies while this window is hidden">🔔 {notify ? 'Notifying' : 'Notify'}</button>
    <button type="button" className={panel ? styles.on : styles.btn} onClick={() => { setPanel(!panel); if (!panel) void loadServices(); }} title="The services running on your PC">⚙ Services</button>
    {note ? <span className={styles.note}>{note}</span> : null}
    {panel ? <div className={styles.panel}>
      {services.map((s) => <div key={s.key} className={styles.row}>
        <span className={s.up ? styles.up : styles.down}>{s.up ? '●' : '○'}</span>
        <span className={styles.name} title={s.what}>{s.name}</span>
        <span className={styles.port}>:{s.port}</span>
        {s.restartable ? <button type="button" className={styles.btn} disabled={busyKey !== null} onClick={() => void restart(s.key)}>{busyKey === s.key ? 'Restarting…' : 'Restart'}</button> : null}
      </div>)}
      {services.length === 0 ? <span className={styles.note}>Loading…</span> : null}
    </div> : null}
  </div>;
}
