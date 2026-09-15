// Verify the deployed /apocrypha bundle contains the nav markers. Read-only.
const base = 'https://www.apocky.com';
const html = await (await fetch(`${base}/apocrypha`)).text();
const scripts = [...new Set([...html.matchAll(/(\/_next\/static\/[^"'\s)]+\.js)/g)].map((m) => m[1]))];
console.log(`html bytes=${html.length} chunks=${scripts.length} buildId=${html.match(/"buildId":"([^"]+)"/)?.[1] ?? '?'}`);
let found = false;
for (const s of scripts) {
  const js = await (await fetch(base + s)).text();
  if (js.includes('chat-site-nav')) {
    found = true;
    console.log(`FOUND in ${s}: Home=${js.includes('"Home"')} Account=${js.includes('"Account"')} close=${js.includes('Close conversations')} bytes=${js.length}`);
  }
}
if (!found) { console.log('NOT FOUND in referenced chunks'); console.log(scripts.join('\n')); }
