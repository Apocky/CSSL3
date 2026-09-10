import { useEffect, useRef } from 'react';

import styles from './CyberDreamField.module.css';

export type DreamActivity = 'idle' | 'listening' | 'thinking' | 'resolved';
export type DreamVariant = 'commons' | 'relay' | 'clearing' | 'atlas';

type Props = {
  activity?: DreamActivity;
  className?: string;
  density?: number;
  variant?: DreamVariant;
  viewport?: boolean;
};

type Point = {
  x: number;
  y: number;
  baseX: number;
  baseY: number;
  phase: number;
  size: number;
};

type Stream = {
  x: number;
  y: number;
  speed: number;
  length: number;
  phase: number;
};

type Pulse = { x: number; y: number; born: number };

const GLYPHS = '§∞◇◈⟡⌁⟲⋮01∆∴';
const PALETTES: Record<DreamVariant, { primary: string; secondary: string; hot: string }> = {
  commons: { primary: '120, 166, 255', secondary: '200, 116, 255', hot: '255, 103, 210' },
  relay: { primary: '94, 214, 255', secondary: '173, 111, 255', hot: '255, 177, 96' },
  clearing: { primary: '123, 192, 255', secondary: '255, 126, 211', hot: '255, 194, 106' },
  atlas: { primary: '148, 129, 255', secondary: '80, 220, 255', hot: '255, 112, 190' },
};

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function seeded(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 4294967296;
  };
}

/**
 * Apocky-native synthesis of interaction ideas explored by Canvas UI:
 * Grid + Glyph Rain + Force Field + Particle Reveal.
 * This implementation is original, dependency-free, and keeps real HTML above
 * the canvas so interaction and fallback behavior do not depend on experimental
 * html-in-canvas browser support.
 * Reference: https://canvasui.dev/ (David Haz, MIT + Commons Clause).
 */
export default function CyberDreamField({
  activity = 'idle',
  className = '',
  density = 1,
  variant = 'commons',
  viewport = false,
}: Props): JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const activityRef = useRef(activity);
  const densityRef = useRef(density);
  const reducedRedrawRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    activityRef.current = activity;
    reducedRedrawRef.current?.();
  }, [activity]);
  useEffect(() => { densityRef.current = density; }, [density]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const host = canvas?.parentElement;
    const context = canvas?.getContext('2d', { alpha: true });
    if (!canvas || !host || !context) return undefined;

    const palette = PALETTES[variant];
    const motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    let reduced = motionQuery.matches;
    const coarse = window.matchMedia('(pointer: coarse)').matches;
    const lean = (navigator.hardwareConcurrency || 4) <= 4;
    let fps = reduced ? 1 : lean || coarse ? 24 : 36;
    let frameInterval = 1000 / fps;
    const pointer = { x: -1000, y: -1000, active: false };
    const pulses: Pulse[] = [];
    let points: Point[] = [];
    let streams: Stream[] = [];
    let width = 1;
    let height = 1;
    let ratio = 1;
    let raf = 0;
    let previous = 0;
    let visible = true;

    const rebuild = () => {
      const rect = canvas.getBoundingClientRect();
      width = Math.max(1, Math.round(rect.width));
      height = Math.max(1, Math.round(rect.height));
      ratio = Math.min(window.devicePixelRatio || 1, lean ? 1.25 : 1.75);
      canvas.width = Math.round(width * ratio);
      canvas.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);

      const rand = seeded(width * 31 + height * 17 + variant.length * 101);
      const pointCount = Math.round(clamp((width * height) / 16500, 34, 112) * clamp(densityRef.current, 0.4, 1.8));
      points = Array.from({ length: pointCount }, () => {
        const x = rand() * width;
        const y = rand() * height;
        return { x, y, baseX: x, baseY: y, phase: rand() * Math.PI * 2, size: 0.45 + rand() * 1.35 };
      });
      const streamCount = Math.round(clamp(width / 105, 6, 22) * clamp(densityRef.current, 0.5, 1.5));
      streams = Array.from({ length: streamCount }, () => ({
        x: rand() * width,
        y: rand() * height,
        speed: 10 + rand() * 28,
        length: 3 + Math.round(rand() * 7),
        phase: rand() * GLYPHS.length,
      }));
    };

    const activityScale = () => {
      switch (activityRef.current) {
        case 'listening': return 1.2;
        case 'thinking': return 1.75;
        case 'resolved': return 1.35;
        default: return 0.82;
      }
    };

    const drawGrid = (time: number, energy: number) => {
      const horizon = height * 0.32;
      const vanishingX = width * 0.5 + Math.sin(time * 0.00008) * width * 0.08;
      context.save();
      context.globalCompositeOperation = 'screen';
      context.lineWidth = 0.65;

      for (let index = -9; index <= 9; index += 1) {
        const foot = width * 0.5 + index * width * 0.095;
        const distance = Math.hypot(pointer.x - foot, pointer.y - height);
        const charge = pointer.active ? Math.max(0, 1 - distance / 540) * energy : 0;
        const bend = (foot - pointer.x) * charge * -0.11;
        context.strokeStyle = `rgba(${index % 3 === 0 ? palette.secondary : palette.primary}, ${0.055 + charge * 0.12})`;
        context.beginPath();
        context.moveTo(vanishingX, horizon);
        context.quadraticCurveTo((vanishingX + foot) * 0.5 + bend, height * 0.66, foot, height + 20);
        context.stroke();
      }

      for (let row = 0; row < 13; row += 1) {
        const normalized = row / 12;
        const eased = normalized * normalized;
        const y = horizon + eased * (height - horizon + 30);
        const sway = Math.sin(time * 0.00035 + row * 0.72) * 3 * energy;
        context.strokeStyle = `rgba(${row % 4 === 0 ? palette.secondary : palette.primary}, ${0.035 + normalized * 0.055})`;
        context.beginPath();
        context.moveTo(0, y + sway);
        context.quadraticCurveTo(width * 0.5, y - 5 * energy, width, y - sway);
        context.stroke();
      }
      context.restore();
    };

    const drawConstellation = (time: number, energy: number) => {
      const influence = activityRef.current === 'thinking' ? 265 : 205;
      context.save();
      context.globalCompositeOperation = 'lighter';

      points.forEach((point, index) => {
        const drift = 4 + point.size * 2;
        point.x = point.baseX + Math.sin(time * 0.00016 + point.phase) * drift;
        point.y = point.baseY + Math.cos(time * 0.00013 + point.phase) * drift;
        if (pointer.active) {
          const dx = point.x - pointer.x;
          const dy = point.y - pointer.y;
          const distance = Math.max(1, Math.hypot(dx, dy));
          const force = Math.max(0, 1 - distance / influence) * 26 * energy;
          point.x += (dx / distance) * force;
          point.y += (dy / distance) * force;
        }

        for (let otherIndex = index + 1; otherIndex < points.length; otherIndex += 1) {
          const other = points[otherIndex];
          if (!other) continue;
          const distance = Math.hypot(point.x - other.x, point.y - other.y);
          if (distance < 118) {
            context.strokeStyle = `rgba(${palette.primary}, ${(1 - distance / 118) * 0.065 * energy})`;
            context.lineWidth = 0.5;
            context.beginPath();
            context.moveTo(point.x, point.y);
            context.lineTo(other.x, other.y);
            context.stroke();
          }
        }

        const glow = context.createRadialGradient(point.x, point.y, 0, point.x, point.y, 9 * point.size);
        glow.addColorStop(0, `rgba(${index % 5 === 0 ? palette.secondary : palette.primary}, ${0.34 * energy})`);
        glow.addColorStop(1, `rgba(${palette.primary}, 0)`);
        context.fillStyle = glow;
        context.beginPath();
        context.arc(point.x, point.y, 9 * point.size, 0, Math.PI * 2);
        context.fill();
      });
      context.restore();
    };

    const drawGlyphs = (time: number, delta: number, energy: number) => {
      context.save();
      context.globalCompositeOperation = 'screen';
      context.textAlign = 'center';
      context.font = `${coarse ? 9 : 10}px ui-monospace, SFMono-Regular, Consolas, monospace`;
      streams.forEach((stream, streamIndex) => {
        stream.y += stream.speed * delta * 0.001 * energy;
        if (stream.y - stream.length * 17 > height) stream.y = -20;
        for (let glyphIndex = 0; glyphIndex < stream.length; glyphIndex += 1) {
          const y = stream.y - glyphIndex * 17;
          const alpha = (1 - glyphIndex / stream.length) * (activityRef.current === 'thinking' ? 0.18 : 0.09);
          const glyph = GLYPHS[(Math.floor(stream.phase + glyphIndex + time * 0.0014 + streamIndex) % GLYPHS.length + GLYPHS.length) % GLYPHS.length] ?? '·';
          context.fillStyle = `rgba(${glyphIndex === 0 ? palette.hot : palette.secondary}, ${alpha})`;
          context.shadowColor = `rgba(${palette.secondary}, ${alpha})`;
          context.shadowBlur = glyphIndex === 0 ? 9 : 2;
          context.fillText(glyph, stream.x + Math.sin(time * 0.0007 + stream.phase) * 5, y);
        }
      });
      context.restore();
    };

    const drawPulses = (time: number, energy: number) => {
      context.save();
      context.globalCompositeOperation = 'screen';
      for (let index = pulses.length - 1; index >= 0; index -= 1) {
        const current = pulses[index];
        if (!current) continue;
        const age = time - current.born;
        if (age > 1450) {
          pulses.splice(index, 1);
          continue;
        }
        const progress = age / 1450;
        const radius = 18 + progress * 310;
        context.strokeStyle = `rgba(${index % 2 ? palette.hot : palette.primary}, ${(1 - progress) * 0.34 * energy})`;
        context.lineWidth = 1.4 - progress;
        context.beginPath();
        context.arc(current.x, current.y, radius, 0, Math.PI * 2);
        context.stroke();
      }
      context.restore();
    };

    const render = (time: number, delta = frameInterval) => {
      context.clearRect(0, 0, width, height);
      const energy = activityScale();
      const haloX = pointer.active ? pointer.x : width * (0.52 + Math.sin(time * 0.00007) * 0.09);
      const haloY = pointer.active ? pointer.y : height * 0.38;
      const halo = context.createRadialGradient(haloX, haloY, 0, haloX, haloY, Math.max(width, height) * 0.58);
      halo.addColorStop(0, `rgba(${palette.secondary}, ${0.055 * energy})`);
      halo.addColorStop(0.42, `rgba(${palette.primary}, ${0.022 * energy})`);
      halo.addColorStop(1, 'rgba(0,0,0,0)');
      context.fillStyle = halo;
      context.fillRect(0, 0, width, height);
      drawGrid(time, energy);
      drawConstellation(time, energy);
      drawGlyphs(time, delta, energy);
      drawPulses(time, energy);
    };
    const redrawReduced = () => {
      if (reduced) render(performance.now(), frameInterval);
    };
    reducedRedrawRef.current = redrawReduced;

    const frame = (time: number) => {
      if (!visible) return;
      if (time - previous >= frameInterval) {
        const delta = previous ? Math.min(time - previous, 100) : frameInterval;
        previous = time;
        render(time, delta);
      }
      if (!reduced) raf = window.requestAnimationFrame(frame);
    };

    const coordinates = (event: PointerEvent) => {
      const rect = canvas.getBoundingClientRect();
      pointer.x = event.clientX - rect.left;
      pointer.y = event.clientY - rect.top;
      pointer.active = true;
    };
    const leave = () => { pointer.active = false; };
    const pulse = (event: PointerEvent) => {
      coordinates(event);
      pulses.push({ x: pointer.x, y: pointer.y, born: performance.now() });
      if (pulses.length > 5) pulses.shift();
    };

    const resize = new ResizeObserver(rebuild);
    const intersection = new IntersectionObserver((entries) => {
      visible = entries[0]?.isIntersecting ?? true;
      if (visible) previous = performance.now();
      if (visible && !reduced && !raf) raf = window.requestAnimationFrame(frame);
      if (!visible && raf) {
        window.cancelAnimationFrame(raf);
        raf = 0;
      }
    });
    const onMotionPreference = () => {
      reduced = motionQuery.matches;
      fps = reduced ? 1 : lean || coarse ? 24 : 36;
      frameInterval = 1000 / fps;
      previous = performance.now();
      if (reduced) {
        if (raf) window.cancelAnimationFrame(raf);
        raf = 0;
        render(previous, frameInterval);
      } else if (visible && !raf) {
        raf = window.requestAnimationFrame(frame);
      }
    };
    rebuild();
    resize.observe(canvas);
    intersection.observe(canvas);
    window.addEventListener('resize', rebuild, { passive: true });
    motionQuery.addEventListener('change', onMotionPreference);
    if (!coarse) host.addEventListener('pointermove', coordinates, { passive: true });
    host.addEventListener('pointerleave', leave, { passive: true });
    host.addEventListener('pointerdown', pulse, { passive: true });
    if (reduced) render(0);
    else raf = window.requestAnimationFrame(frame);

    return () => {
      if (reducedRedrawRef.current === redrawReduced) reducedRedrawRef.current = null;
      if (raf) window.cancelAnimationFrame(raf);
      resize.disconnect();
      intersection.disconnect();
      window.removeEventListener('resize', rebuild);
      motionQuery.removeEventListener('change', onMotionPreference);
      host.removeEventListener('pointermove', coordinates);
      host.removeEventListener('pointerleave', leave);
      host.removeEventListener('pointerdown', pulse);
    };
  }, [density, variant]);

  return (
    <canvas
      ref={canvasRef}
      className={`${styles.canvas} ${viewport ? styles.viewport : ''} ${className}`}
      aria-hidden="true"
      data-canvasui-synthesis="grid+glyph-rain+force-field+particle-reveal"
    />
  );
}
