// Fingerprint which branch's ChatThread is live on apocky.com. Read-only.
const base = 'https://www.apocky.com';
const html = await (await fetch(`${base}/apocrypha`)).text();
const scripts = [...new Set([...html.matchAll(/(\/_next\/static\/[^"'\s)]+\.js)/g)].map((m) => m[1]))];
const markers = {
  'desktop branch (Apocrypha node)': 'Waiting for the local Apocrypha node',
  'wip/ckpt branch (Qwen node)': 'Waiting for the local Qwen node',
  'desktop new-chat key': 'apocky.apocrypha.new-chat.v1',
  'nav fix (chat-site-nav)': 'chat-site-nav',
};
for (const s of scripts) {
  const js = await (await fetch(base + s)).text();
  for (const [label, needle] of Object.entries(markers)) if (js.includes(needle)) console.log(`${label.padEnd(34)} PRESENT in ${s.split('/').pop()}`);
}
