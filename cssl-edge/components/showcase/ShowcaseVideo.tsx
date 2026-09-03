import { useEffect, useRef, useState } from 'react';

import styles from '../../styles/Showcase.module.css';

type VideoFormat = 'landscape' | 'vertical';

const VIDEO = {
  landscape: {
    label: 'Landscape 16:9',
    src: '/showcase/promo-apocky-chaos-landscape-23s-v1.mp4',
    poster: '/showcase/promo-apocky-chaos-landscape-cover-v1.png',
  },
  vertical: {
    label: 'Portrait 9:16',
    src: '/showcase/promo-apocky-chaos-vertical-23s-v1.mp4',
    poster: '/showcase/promo-apocky-chaos-vertical-cover-v1.png',
  },
} as const;

const PORTRAIT_QUERY = '(max-width: 820px) and (orientation: portrait)';

export default function ShowcaseVideo(): JSX.Element {
  const [format, setFormat] = useState<VideoFormat>('landscape');
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(23);
  const manualChoice = useRef(false);
  const player = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const query = window.matchMedia(PORTRAIT_QUERY);
    const chooseForScreen = () => {
      if (!manualChoice.current) setFormat(query.matches ? 'vertical' : 'landscape');
    };

    chooseForScreen();
    if (typeof query.addEventListener === 'function') {
      query.addEventListener('change', chooseForScreen);
      return () => query.removeEventListener('change', chooseForScreen);
    }

    query.addListener(chooseForScreen);
    return () => query.removeListener(chooseForScreen);
  }, []);

  const choose = (next: VideoFormat) => {
    manualChoice.current = true;
    player.current?.pause();
    setPlaying(false);
    setCurrentTime(0);
    setFormat(next);
  };

  const active = VIDEO[format];

  const togglePlayback = () => {
    const video = player.current;
    if (!video) return;
    if (video.paused) {
      void video.play().catch(() => setPlaying(false));
    } else {
      video.pause();
    }
  };

  const toggleMute = () => {
    const video = player.current;
    if (!video) return;
    video.muted = !video.muted;
    setMuted(video.muted);
  };

  const enterFullscreen = () => {
    const video = player.current as (HTMLVideoElement & { webkitEnterFullscreen?: () => void }) | null;
    if (!video) return;
    if (typeof video.webkitEnterFullscreen === 'function') {
      video.webkitEnterFullscreen();
      return;
    }
    void video.requestFullscreen?.();
  };

  const formatTime = (seconds: number) => {
    const safe = Number.isFinite(seconds) && seconds >= 0 ? seconds : 0;
    return `${Math.floor(safe / 60)}:${Math.floor(safe % 60).toString().padStart(2, '0')}`;
  };

  return (
    <figure className={styles.videoFigure}>
      <div className={styles.videoToolbar}>
        <div>
          <span className={styles.statusDot} aria-hidden="true" />
          <span>23-second film · captions included</span>
        </div>
        <fieldset className={styles.formatSwitch}>
          <legend className={styles.srOnly}>Choose video format</legend>
          {(Object.keys(VIDEO) as VideoFormat[]).map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={format === option}
              onClick={() => choose(option)}
            >
              {VIDEO[option].label}
            </button>
          ))}
        </fieldset>
      </div>

      <div className={styles.videoStage} data-format={format}>
        <video
          key={format}
          ref={player}
          className={styles.video}
          src={active.src}
          poster={active.poster}
          playsInline
          preload="metadata"
          aria-label={`Play the Apocky and Chaos Tarot connected-worlds showcase in ${active.label} format`}
          aria-describedby="showcase-video-note showcase-art-disclosure"
          onLoadedMetadata={(event) => {
            const seconds = event.currentTarget.duration;
            if (Number.isFinite(seconds) && seconds > 0) setDuration(seconds);
          }}
          onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          onEnded={() => setPlaying(false)}
          onVolumeChange={(event) => setMuted(event.currentTarget.muted)}
        >
          <track
            default
            kind="captions"
            src="/showcase/promo-apocky-chaos-23s-en-v1.vtt"
            srcLang="en"
            label="English"
          />
          Your browser cannot play this video. Read the transcript below or use the direct project links.
        </video>
      </div>

      <div className={styles.customControls} role="group" aria-label="Video controls">
        <button type="button" onClick={togglePlayback} aria-label={playing ? 'Pause video' : 'Play video'}>
          <span aria-hidden="true">{playing ? 'Ⅱ' : '▶'}</span>
          {playing ? 'Pause' : 'Play'}
        </button>
        <label className={styles.timeline}>
          <span className={styles.srOnly}>Video progress</span>
          <input
            type="range"
            min="0"
            max={duration}
            step="0.1"
            value={Math.min(currentTime, duration)}
            aria-label="Video progress"
            aria-valuetext={`${formatTime(currentTime)} of ${formatTime(duration)}`}
            onChange={(event) => {
              const next = Number(event.currentTarget.value);
              if (player.current && Number.isFinite(next)) {
                player.current.currentTime = next;
                setCurrentTime(next);
              }
            }}
          />
        </label>
        <output className={styles.timestamp} aria-label="Playback time">
          {formatTime(currentTime)} / {formatTime(duration)}
        </output>
        <button type="button" onClick={toggleMute} aria-label={muted ? 'Unmute video' : 'Mute video'}>
          <span aria-hidden="true">{muted ? '◇' : '◈'}</span>
          {muted ? 'Unmute' : 'Mute'}
        </button>
        <button type="button" onClick={enterFullscreen} aria-label="Open video full screen">
          <span aria-hidden="true">⛶</span>
          Full screen
        </button>
      </div>

      <figcaption id="showcase-video-note" className={styles.videoNote}>
        Playback starts only when you choose it. The film contains ambient instrumental audio and no voice-over.
        Captions are visible in the film and exposed as an English caption track.
      </figcaption>
    </figure>
  );
}
