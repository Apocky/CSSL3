// The sampling dials, as a tool.
//
// They were only ever reachable by dragging six sliders in the window, which meant the one person
// who knows which temperament a task wants -- the model being asked to do it -- could not touch
// them, and the operator had to translate "be more careful here" into a number. Now it is a tool
// call: ask for precise, get precise, and the sliders move to match.
//
// The numbers live in lib/apocrypha/sampling.ts and nowhere else. This file resolves and clamps
// through that module rather than restating any of them, so the tool cannot drift from the preset
// the window shows or from what the engine is actually sent.
import {
  SAMPLING_PRESETS,
  resolveSampling,
  type PresetId,
  type SamplingProfile,
} from '../../../lib/apocrypha/sampling';
import type { ToolDefinition } from '../types';

const PRESET_IDS = SAMPLING_PRESETS.map((preset) => preset.id);

/** The dials worth naming to a caller. The rest of the surface stays inside the preset. */
const OVERRIDABLE = ['temperature', 'topP', 'topK', 'minP', 'repeatPenalty', 'dryMultiplier'] as const;

export interface DialState {
  preset: PresetId;
  overrides: Record<string, number>;
  profile: SamplingProfile;
}

export const DIAL_TOOLS: readonly ToolDefinition[] = [
  {
    name: 'set_dials',
    // 'read' on purpose: this writes no file, runs no command, and reaches nothing outside the
    // turn. Gating it behind a consent prompt would make "be more careful here" cost a click.
    risk: 'read',
    description: [
      'Change how you sample the next reply: the temperament of the answer, not its content.',
      'Prefer a named preset; reach for individual dials only when the preset is close but wrong in one respect.',
      SAMPLING_PRESETS.map((preset) => `  ${preset.id} -- ${preset.effect}`).join('\n'),
      'The change takes effect on your very next completion and persists for the rest of the session.',
    ].join('\n'),
    parameters: {
      type: 'object',
      properties: {
        preset: { type: 'string', enum: [...PRESET_IDS], description: 'Temperament to adopt.' },
        temperature: { type: 'number', description: '0-2. Higher = likelier to pick an unlikely word.' },
        topP: { type: 'number', description: '0-1. Keep the smallest set of words worth this much probability.' },
        topK: { type: 'number', description: '0-200. Never consider more than this many words.' },
        minP: { type: 'number', description: '0-1. Drop words this much less likely than the best one.' },
        repeatPenalty: { type: 'number', description: '0.5-2. Blunt: penalises every repeated token, code syntax included.' },
        dryMultiplier: { type: 'number', description: '0-3. How hard to push back on a repeated phrase.' },
        reason: { type: 'string', description: 'One short line on why this temperament suits the task. Shown to the operator.' },
      },
      required: [],
    },
  },
  {
    name: 'get_dials',
    risk: 'read',
    description: 'Report the sampling dials currently in force, and the presets available.',
    parameters: { type: 'object', properties: {}, required: [] },
  },
];

export function ownsDialTool(name: string): boolean {
  return DIAL_TOOLS.some((tool) => tool.name === name);
}

function describe(state: DialState): string {
  const preset = SAMPLING_PRESETS.find((candidate) => candidate.id === state.preset);
  const dials = OVERRIDABLE
    .map((key) => [key, (state.profile as unknown as Record<string, unknown>)[key]] as const)
    .filter(([, value]) => typeof value === 'number')
    .map(([key, value]) => `${key}=${value}`)
    .join(' ');
  return `${preset?.label ?? state.preset}: ${dials}`;
}

/**
 * Applies the call to `state` IN PLACE and returns what the model is told.
 *
 * In place, because the point is that the next completion of this same turn already samples
 * differently -- a dial that only takes effect next turn is a setting, not an instrument.
 */
export function runDialTool(
  name: string,
  args: Record<string, unknown>,
  state: DialState,
): { summary: string; content: string; changed: boolean } {
  if (name === 'get_dials') {
    return {
      summary: describe(state),
      content: [
        `in force: ${describe(state)}`,
        '',
        'presets:',
        ...SAMPLING_PRESETS.map((preset) => `  ${preset.id} -- ${preset.effect}`),
      ].join('\n'),
      changed: false,
    };
  }

  const requested = typeof args.preset === 'string' ? args.preset : state.preset;
  const known = PRESET_IDS.includes(requested as PresetId);
  if (typeof args.preset === 'string' && !known) {
    throw new Error(`unknown preset "${args.preset}"; choose one of ${PRESET_IDS.join(', ')}`);
  }

  // A named preset is a fresh start, not a layer over the last set of overrides: asking for
  // `balanced` after nudging the temperature should GIVE you balanced.
  const overrides: Record<string, number> = typeof args.preset === 'string' ? {} : { ...state.overrides };
  for (const key of OVERRIDABLE) {
    const value = args[key];
    if (typeof value === 'number' && Number.isFinite(value)) overrides[key] = value;
  }

  state.preset = (known ? requested : state.preset) as PresetId;
  state.overrides = overrides;
  // resolveSampling CLAMPS rather than throws, so a model that asks for temperature 9 gets 2 and a
  // working turn instead of a dead one.
  state.profile = resolveSampling(state.preset, overrides);

  const reason = typeof args.reason === 'string' && args.reason.trim() ? ` (${args.reason.trim().slice(0, 160)})` : '';
  return {
    summary: `${describe(state)}${reason}`,
    content: `dials now ${describe(state)}. This applies to your next completion.`,
    changed: true,
  };
}
