// Voice on the PC: speech-to-text with faster-whisper (CPU, base.en; measured 0.75 s, 7x realtime)
// and text-to-speech with Windows SAPI (0.23 s). Both hidden child processes; no GPU contention.
import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PYTHON = process.env.APOCRYPHA_PYTHON ?? 'C:\Python314\python.exe';
const FFMPEG_DIR = 'C:\Users\Apocky\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-8.1-full_build\bin';

function run(file: string, args: string[], timeout: number): Promise<{ out: string; err: string; code: number }> {
  return new Promise((resolve) => {
    execFile(file, args, { windowsHide: true, timeout, maxBuffer: 16 * 1024 * 1024, env: { ...process.env, PATH: `${FFMPEG_DIR};${process.env.PATH ?? ''}` } },
      (error, out, err) => resolve({ out: String(out), err: String(err), code: error ? 1 : 0 }));
  });
}

const STT = `import sys, json
from faster_whisper import WhisperModel
m = WhisperModel("base.en", device="cpu", compute_type="int8")
segs, _ = m.transcribe(sys.argv[1], vad_filter=True)
print(json.dumps({"text": " ".join(s.text.strip() for s in segs)}))`;

export async function transcribe(audio: Buffer, ext = 'webm'): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'apx-stt-'));
  try {
    const file = join(dir, `clip.${ext.replace(/[^a-z0-9]/giu, '') || 'webm'}`);
    await writeFile(file, audio);
    const { out, err, code } = await run(PYTHON, ['-c', STT, file], 120_000);
    if (code !== 0) throw new Error(`speech-to-text failed: ${err.slice(-300)}`);
    return (JSON.parse(out.trim().split('\n').pop() ?? '{}') as { text?: string }).text ?? '';
  } finally { await rm(dir, { recursive: true, force: true }); }
}

export async function speak(text: string): Promise<Buffer> {
  const dir = await mkdtemp(join(tmpdir(), 'apx-tts-'));
  try {
    const src = join(dir, 'say.txt'); const wav = join(dir, 'say.wav');
    await writeFile(src, text.slice(0, 6_000), 'utf8');
    const script = `Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.SetOutputToWaveFile('${wav}'); $s.Speak([IO.File]::ReadAllText('${src}')); $s.Dispose()`;
    const { err, code } = await run('powershell.exe', ['-NoProfile', '-NonInteractive', '-WindowStyle', 'Hidden', '-Command', script], 120_000);
    if (code !== 0) throw new Error(`text-to-speech failed: ${err.slice(-300)}`);
    return await readFile(wav);
  } finally { await rm(dir, { recursive: true, force: true }); }
}
