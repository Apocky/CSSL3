import { HALOIC_VOCAB } from '../../lib/spellcraft';

export const INTENT_PRESETS = [
  { label: 'Clarity', source: 'ka-ken-el', meaning: 'Knowledge and illumination', prompt: 'What is one thing you could understand more clearly today?' },
  { label: 'Boundaries', source: 'nau zur', meaning: 'A limit worth protecting', prompt: 'Where would a clear, kind “no” make more room for you?' },
  { label: 'Growth', source: 'na-ber-el', meaning: 'Creation and becoming', prompt: 'What small beginning deserves your attention?' },
  { label: 'Balance', source: 'man om', meaning: 'A moment to come back to yourself', prompt: 'What needs more of your attention, and what needs less?' },
  { label: 'Release', source: 'sha ban', meaning: 'Letting a shadow leave your focus', prompt: 'What can you stop carrying into the next moment?' },
  { label: 'Renewal', source: 'ur-lif-el', meaning: 'Returning to life and vitality', prompt: 'What helps you feel restored, even in a small way?' },
] as const;
export const THEMES = HALOIC_VOCAB.filter(entry => entry.namespace === 'root');
export const ACTIONS = [{ value: 'aspire', label: 'Move toward' }, { value: 'create', label: 'Bring forth' }, { value: 'balance', label: 'Find balance in' }, { value: 'protect', label: 'Protect' }, { value: 'release', label: 'Let go of' }] as const;

export function selectedIntent(input: string): typeof INTENT_PRESETS[number] | undefined {
  return INTENT_PRESETS.find(preset => preset.source === input.trim());
}
export function buildIntent(action: string, theme: string): string {
  if (!THEMES.some(entry => entry.lexeme === theme)) throw new Error('Choose a listed meaning.');
  const root = `root:${theme}`;
  if (action === 'aspire') return `na-${root}-el`;
  if (action === 'create') return `ka-${root}-el`;
  if (action === 'balance') return `${root} om`;
  if (action === 'protect') return `${root} zur`;
  if (action === 'release') return `${root} ban`;
  throw new Error('Choose a listed action.');
}
