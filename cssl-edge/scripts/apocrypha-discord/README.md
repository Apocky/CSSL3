# Apocrypha on Discord

Talk to Apocrypha from Discord — on the desktop, on your phone, or through the overlay while a
game is running. Same mind and same memory as apocky.com, because it calls the same local mind
service rather than standing up a second brain.

Zero dependencies. Node 25 has a WebSocket and a fetch, which is everything a gateway client
needs.

---

## What you have to do

Three things need a real Discord account, so they are yours. Everything else is built.

**1. Create the application** — <https://discord.com/developers/applications> → *New Application*.
Call it Apocrypha. Under **Bot**, set the name and avatar you want people to see.

**2. Turn on the message content intent** — same page, **Bot** → *Privileged Gateway Intents* →
**MESSAGE CONTENT INTENT** → on.

> Skip this and the bot connects fine, then receives every guild message with an **empty**
> `content` field. It looks like Apocrypha ignoring you rather than a missing checkbox. If the
> intent is off, Discord closes the socket with code **4014** and the bridge prints that exact
> fix instead of retrying forever.

**3. Copy the token and your user id into an env file.**

- Token: **Bot** → *Reset Token* → Copy.
- Your user id: Discord → Settings → Advanced → Developer Mode on, then right-click your own
  name → *Copy User ID*.

```
copy scripts\apocrypha-discord\discord.env.example D:\Apocrypha\discord.env
```

Fill in `APOCRYPHA_DISCORD_TOKEN`, `APOCRYPHA_DISCORD_OWNER_ID`, and
`APOCRYPHA_DISCORD_APPLICATION_ID`. The file lives outside the repo so a token cannot be
committed by accident.

**The token never passes through me.** I do not create Discord applications and I do not handle
credentials. If one ever appears in a chat message, reset it in the portal — that is free and
instant.

---

## Then check it

```
node --env-file=D:\Apocrypha\discord.env --import tsx scripts/apocrypha-discord/runner.ts --check
```

Validates the config, probes the mind service, and prints a ready-made invite link with the
minimum permission set. It does **not** open a gateway connection, so a mistake costs nothing.

Open the invite link to add Apocrypha to a server. DMs work without it.

---

## What it will and will not read

From `specs/discord/CONSENT_POLICY.md`, enforced in `policy.ts` and proven in
`tests/discord/policy.test.ts`:

| Situation | What happens |
| --- | --- |
| You DM it | answers, and remembers |
| Anyone @mentions it in a server | answers, does **not** remember |
| Anyone replies to one of its messages | answers, does **not** remember |
| A channel you listed in `APOCRYPHA_DISCORD_CHANNELS` | answers every message there |
| **Anything else** | **discarded at the socket, unread** |
| Anyone says `stop` to it | never replies to that person again |

A bot holding the message-content intent can see every message in every channel it is in. Most of
those people never agreed to talk to an AI, and a server owner cannot agree on their behalf. So
the default is discard: a message that is not addressed to Apocrypha is dropped before it is
parsed into anything the mind can see, is never logged, and is never remembered.

Only **your** conversations are remembered. A stranger who mentions it gets a real answer, and
that exchange then ends — it never becomes context for anyone, including them.

The cost of this, stated plainly: Apocrypha cannot spontaneously join a conversation it was not
invited into. That is refused deliberately, not deferred.

`stop` is matched as a whole message, so "don't stop" does not mute you. Only you can lift a
halt, by replying `resume`.

---

## Running it

```
node --env-file=D:\Apocrypha\discord.env --import tsx scripts/apocrypha-discord/runner.ts
```

Registered in the one-click launcher as `discord`, so plain `apocrypha` starts it with everything
else. Health on <http://127.0.0.1:19133/health>, which reports the gateway and the mind
separately — "the bridge is down" and "the bridge is up but its mind is not" are different
problems with different fixes.

### It needs the mind service

The bridge calls `scripts/apocrypha-mind/server.ts`, expected on **19132**. Not 19131: the mind
service's own default is 19131, but that port is probed elsewhere as a stray-second-engine check,
so binding it makes the work lane's doctor report a phantom.

Start the mind with reasoning off, which the standing directive requires and its default does not
do:

```
set APOCRYPHA_MIND_PORT=19132
set APOCRYPHA_MIND_REASONING=0
node --import tsx scripts/apocrypha-mind/server.ts
```

If the mind is down, Discord still connects and says so in plain words rather than going quiet.
It deliberately does **not** fall back to the raw engine: an answer with no persona and no memory
would be a different entity wearing the same name.

---

## Files

| File | What it is |
| --- | --- |
| `gateway.ts` | the protocol: handshake, jittered heartbeat, resume, zombie detection, fatal-vs-recoverable close codes |
| `policy.ts` | who it is allowed to hear. Pure, so the tests enumerate the whole decision space |
| `rest.ts` | sending, typing, the 2000-char split that survives code fences, 429 backoff |
| `mind.ts` | the call into the local mind service, and honest failures when it is down |
| `bridge.ts` | wiring, per-channel history, halts, one introduction per channel |
| `runner.ts` | entrypoint, `--check`, health endpoint |

## Tests

```
npm run test:discord
```

42 tests, no token required. The gateway tests run against a real loopback WebSocket server that
speaks the Discord protocol (`tests/discord/fake-gateway.ts`) — mocking the WebSocket class would
only test my idea of what it does.

The consent gate is mutation-tested: opening the gate, leaking message text into the audit log,
and remembering strangers each make the suite fail. A check never observed failing is not a check.

## Not in this version

**Voice.** Discord voice needs a raw UDP socket plus Opus encoding and XChaCha20-Poly1305, none of
which Node provides dependency-free. It is a real build, not a flag, so it is not pretended at
here. Speech on this machine is already measured as feasible (Whisper at 7× realtime on CPU, SAPI
for output); joining a Discord voice channel is the missing piece.
