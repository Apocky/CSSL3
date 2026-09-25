// Line icons for the room's controls. Stroke-only, currentColor, 20px box, decorative (the
// controls carry their own aria-labels).

type P = { readonly size?: number };
const base = (size = 20) => ({
  width: size, height: size, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor',
  strokeWidth: 1.8, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const, 'aria-hidden': true,
});

export const PlusIcon = ({ size }: P) => <svg {...base(size)}><path d="M12 5v14M5 12h14" /></svg>;
export const SendIcon = ({ size }: P) => <svg {...base(size)}><path d="M12 19V5M5.5 11.5 12 5l6.5 6.5" /></svg>;
export const StopIcon = ({ size }: P) => <svg {...base(size)}><rect x="7" y="7" width="10" height="10" rx="1.5" fill="currentColor" /></svg>;
export const GearIcon = ({ size }: P) => <svg {...base(size)}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.6-1.1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" /></svg>;
export const SpeakerIcon = ({ size }: P) => <svg {...base(size)}><path d="M11 5 6 9H3v6h3l5 4V5Z" /><path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" /></svg>;
export const MutedIcon = ({ size }: P) => <svg {...base(size)}><path d="M11 5 6 9H3v6h3l5 4V5Z" /><path d="m22 9-6 6M16 9l6 6" /></svg>;
export const CameraIcon = ({ size }: P) => <svg {...base(size)}><path d="M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1Z" /><circle cx="12" cy="13" r="3.5" /></svg>;
export const PhotoIcon = ({ size }: P) => <svg {...base(size)}><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="8.5" cy="9.5" r="1.5" /><path d="m21 16-5-5-8 9" /></svg>;
export const FileIcon = ({ size }: P) => <svg {...base(size)}><path d="M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8Z" /><path d="M14 3v5h5" /></svg>;
export const ImageGenIcon = ({ size }: P) => <svg {...base(size)}><path d="m12 3 1.9 4.6L18.5 9.5l-4.6 1.9L12 16l-1.9-4.6L5.5 9.5l4.6-1.9Z" /><path d="M19 15l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8Z" /></svg>;
export const GlobeIcon = ({ size }: P) => <svg {...base(size)}><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></svg>;
export const LockIcon = ({ size }: P) => <svg {...base(size)}><rect x="5" y="11" width="14" height="10" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" /></svg>;
export const ChevronIcon = ({ size }: P) => <svg {...base(size)}><path d="m6 9 6 6 6-6" /></svg>;
export const CopyIcon = ({ size }: P) => <svg {...base(size)}><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V5a1 1 0 0 1 1-1h10" /></svg>;
export const CheckIcon = ({ size }: P) => <svg {...base(size)}><path d="m5 12 5 5 9-10" /></svg>;
export const DownIcon = ({ size }: P) => <svg {...base(size)}><path d="M12 5v14M5.5 12.5 12 19l6.5-6.5" /></svg>;
export const CloseIcon = ({ size }: P) => <svg {...base(size)}><path d="M6 6l12 12M18 6 6 18" /></svg>;
