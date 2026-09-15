// Add an authenticator.
//
// Three routes in, because which one is usable depends entirely on where you are standing:
//
//   QR   — the normal case: setting up on a desktop, or scanning with a second device.
//   link — the otpauth:// URI, for when the authenticator is on THIS device. A camera cannot
//          photograph its own screen, so the QR is the one thing that does not work there.
//   key  — typed by hand, or pasted into a password manager, when neither of the above fits.
//
// I shipped this with only the link at first, reasoning from the phone case alone. That was the
// wrong generalisation: the QR is what people expect and what works when the authenticator lives
// on different hardware from the screen.

import { useCallback, useEffect, useState } from 'react';

type Stage = 'idle' | 'starting' | 'showing' | 'confirming' | 'done';

interface Started {
  readonly uri: string;
  readonly qr: string | null;
  readonly secret: string;
  readonly account: string;
}

function grouped(secret: string): string {
  return secret.replace(/(.{4})/gu, '$1 ').trim();
}

export interface AuthenticatorSetupProps {
  /** Called once enrolment is confirmed. Used by the sign-in flow to carry on to the destination. */
  readonly onComplete?: () => void;
  /** Suppresses the standalone "done" panel when a caller shows its own continuation. */
  readonly hideDoneState?: boolean;
}

export function AuthenticatorSetup({ onComplete, hideDoneState }: AuthenticatorSetupProps = {}): JSX.Element {
  const [stage, setStage] = useState<Stage>('idle');
  const [started, setStarted] = useState<Started | null>(null);
  const [code, setCode] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // Whether this account ALREADY has a working authenticator. Without it the panel looked identical
  // whether enrolment had been completed or merely started, so "did that work?" had no answer
  // anywhere in the product — which is how an unconfirmed enrolment survived six sign-in attempts.
  const [enrolled, setEnrolled] = useState<boolean | null>(null);

  useEffect(() => {
    let live = true;
    void fetch('/api/auth/totp/setup', { method: 'GET', cache: 'no-store', credentials: 'same-origin' })
      .then((response) => response.ok ? response.json() : null)
      .then((payload: { ok?: boolean; enrolled?: boolean } | null) => {
        if (live && payload?.ok === true && typeof payload.enrolled === 'boolean') setEnrolled(payload.enrolled);
      })
      .catch(() => { /* status is a courtesy; the panel works without it */ });
    return () => { live = false; };
  }, [stage]);

  const start = useCallback(async () => {
    setStage('starting');
    setNotice(null);
    try {
      const response = await fetch('/api/auth/totp/setup', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({}),
      });
      const payload = await response.json().catch(() => null) as (Started & { error?: string }) | null;
      if (!response.ok || !payload?.uri) {
        setNotice(payload?.error ?? 'Setup could not be started. Try again in a moment.');
        setStage('idle');
        return;
      }
      setStarted({ uri: payload.uri, qr: payload.qr ?? null, secret: payload.secret, account: payload.account });
      setStage('showing');
    } catch {
      setNotice('Setup could not be reached. Try again.');
      setStage('idle');
    }
  }, []);

  const confirm = useCallback(async (event: React.FormEvent) => {
    event.preventDefault();
    const digits = code.replace(/[^0-9]/gu, '');
    if (digits.length !== 6) return;
    setStage('confirming');
    setNotice(null);
    try {
      const response = await fetch('/api/auth/totp/setup', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ code: digits }),
      });
      const payload = await response.json().catch(() => null) as { ok?: boolean; error?: string } | null;
      if (!response.ok || !payload?.ok) {
        setNotice(payload?.error ?? 'That code was not accepted.');
        setStage('showing');
        setCode('');
        return;
      }
      setStage('done');
      setStarted(null);
      setCode('');
      onComplete?.();
    } catch {
      setNotice('Confirmation could not be reached. Try again.');
      setStage('showing');
    }
  }, [code, onComplete]);

  if (stage === 'done' && hideDoneState) {
    return <section className="apx-totp" id="authenticator">
      <h2>Authenticator</h2>
      <p className="apx-totp-ok">Set up. Taking you on…</p>
      <style jsx>{STYLE}</style>
    </section>;
  }

  if (stage === 'done') {
    return <section className="apx-totp" id="authenticator">
      <h2>Authenticator</h2>
      <p className="apx-totp-ok">Set up. From now on you sign in with your email and the 6-digit code.</p>
      <button type="button" onClick={() => { setStage('idle'); setNotice(null); }}>Set up a different authenticator</button>
      <p className="apx-totp-hint">Replacing it invalidates the current one — delete the old Apocky entry in your app.</p>
      <style jsx>{STYLE}</style>
    </section>;
  }

  return <section className="apx-totp" id="authenticator">
    <h2>Authenticator</h2>
    <p className="apx-totp-lede">
      Sign in with a 6-digit code from an authenticator app instead of waiting for an email.
    </p>

    {enrolled === true && stage === 'idle' ? <p className="apx-totp-state apx-totp-state-on" role="status">
      <strong>Set up.</strong> This account signs in with an authenticator code.
    </p> : null}
    {enrolled === false && stage === 'idle' ? <p className="apx-totp-state apx-totp-state-off" role="status">
      <strong>Not set up.</strong> This account has no authenticator yet, so authenticator codes will
      be refused at sign-in until you finish the steps below.
    </p> : null}

    {stage === 'idle' ? (
      <button type="button" className="apx-totp-primary" onClick={() => { void start(); }}>
        Set up an authenticator
      </button>
    ) : null}
    {stage === 'starting' ? <p role="status">Preparing…</p> : null}

    {started && (stage === 'showing' || stage === 'confirming') ? <>
      <ol className="apx-totp-steps">
        {started.qr ? <li>
          Scan this with your authenticator app:
          {/* On a white plate on purpose: scanners cope badly with an inverted code, and this page
              is nearly black. */}
          <img className="apx-totp-qr" src={started.qr} alt={`Authenticator setup code for ${started.account}`} width={320} height={320} />
        </li> : null}
        <li>
          Setting up on this same device? A camera cannot scan its own screen:
          <a className="apx-totp-primary apx-totp-link" href={started.uri}>Add to authenticator app</a>
          <span className="apx-totp-hint">Opens your authenticator and adds {started.account}.</span>
        </li>
        <li>
          Or enter this key by hand:
          <code className="apx-totp-key">{grouped(started.secret)}</code>
          <button
            type="button"
            className="apx-totp-copy"
            onClick={() => {
              void navigator.clipboard?.writeText(started.secret).then(
                () => { setCopied(true); window.setTimeout(() => setCopied(false), 2000); },
                () => setNotice('Copying is blocked here. Select the key and copy it directly.'),
              );
            }}
          >{copied ? 'Copied' : 'Copy key'}</button>
        </li>
        <li className="apx-totp-final">
          <strong>Finish setup.</strong> Type the 6-digit code your authenticator now shows. Scanning
          alone does not switch anything on — until this code is accepted, the account has no
          authenticator and signing in with one will be refused.
          <form onSubmit={confirm} className="apx-totp-confirm">
            <label className="apx-totp-srlabel" htmlFor="totp-confirm">Authenticator code</label>
            <input
              id="totp-confirm"
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={7}
              value={code}
              onChange={(event) => setCode(event.target.value.replace(/[^0-9 ]/gu, ''))}
              placeholder="000000"
            />
            <button type="submit" disabled={stage === 'confirming' || code.replace(/[^0-9]/gu, '').length !== 6}>
              {stage === 'confirming' ? 'Checking…' : 'Confirm'}
            </button>
          </form>
        </li>
      </ol>
      {/* This banner exists because of a real sign-in failure. The QR was scanned, setup was
          started twice, and the code was then typed into the SIGN-IN page — which can only ever
          refuse it, because the secret is still pending. Six rejections later it still read as "the
          authenticator is broken". The step that was missed is the one that has to shout. */}
      <p className="apx-totp-unfinished" role="status">
        Not finished yet. Enter the code <em>here</em>, not on the sign-in page — the sign-in page
        cannot accept it until this step is done.
      </p>
      <p className="apx-totp-hint">
        Nothing changes until you confirm — your current way of signing in keeps working until then.
        Starting setup again replaces this code, so an authenticator that scanned an earlier one
        will stop matching; delete the old Apocky entry if you start over.
      </p>
    </> : null}

    {notice ? <p className="apx-totp-error" role="alert">{notice}</p> : null}
    <style jsx>{STYLE}</style>
  </section>;
}

const STYLE = `
.apx-totp { margin-top: 28px; padding: 18px; border: 1px solid #a9b5ff33; border-radius: 14px; background: #0d1019; }
.apx-totp h2 { margin: 0 0 6px; font-size: 16px; }
.apx-totp-lede { margin: 0 0 14px; color: #a9b5ffc0; font-size: 14px; }
.apx-totp-primary {
  display: inline-block; border: 0; border-radius: 999px; padding: 10px 18px;
  background: #b1dfeb; color: #05060b; font: 600 14px/1 inherit; cursor: pointer; text-decoration: none;
}
.apx-totp-link { line-height: 1.4; }
.apx-totp-steps { margin: 0; padding-left: 20px; display: grid; gap: 16px; font-size: 14px; }
.apx-totp-steps li { line-height: 1.6; }
.apx-totp-qr {
  display: block; margin: 10px 0; width: min(280px, 100%); height: auto;
  background: #fff; padding: 10px; border-radius: 12px;
}
.apx-totp-key {
  display: block; margin: 8px 0; padding: 10px 12px; border-radius: 10px;
  border: 1px solid #a9b5ff40; background: #111524; color: #b1dfeb;
  letter-spacing: .12em; word-break: break-all; font-size: 14px;
}
.apx-totp-copy, .apx-totp-confirm button {
  border: 1px solid #a9b5ff50; border-radius: 999px; padding: 7px 14px;
  background: #111524; color: #d2e6fa; font: inherit; font-size: 13px; cursor: pointer;
}
.apx-totp-confirm { display: flex; gap: 8px; margin-top: 8px; flex-wrap: wrap; }
.apx-totp-confirm input {
  flex: 1 1 140px; min-width: 0; padding: 9px 12px; border-radius: 10px;
  border: 1px solid #a9b5ff40; background: #111524; color: inherit; font: inherit;
  letter-spacing: .18em;
}
.apx-totp-confirm button:disabled { opacity: .45; cursor: default; }
.apx-totp-hint { display: block; margin-top: 6px; color: #a9b5ff90; font-size: 12.5px; }
.apx-totp-final { padding: 12px 14px; border: 1px solid #b1dfeb55; border-left-width: 3px; border-radius: 10px; background: #0f1724; }
.apx-totp-final strong { color: #b1dfeb; }
.apx-totp-unfinished {
  margin: 14px 0 0; padding: 11px 14px; border-radius: 10px;
  border: 1px solid #fbbf2455; background: rgba(251, 191, 36, .08);
  color: #f3e2bd; font-size: 13px; line-height: 1.5;
}
.apx-totp-unfinished em { color: #fbbf24; font-style: normal; font-weight: 600; }
.apx-totp-error { margin-top: 12px; color: #ffb4b4; font-size: 13.5px; }
.apx-totp-ok { color: #b1dfeb; font-size: 14px; }
.apx-totp-state { margin: 0 0 14px; padding: 10px 13px; border-radius: 10px; font-size: 13px; line-height: 1.5; border: 1px solid; }
.apx-totp-state-on { border-color: #34d39955; background: rgba(52, 211, 153, .08); color: #cfe9df; }
.apx-totp-state-on strong { color: #34d399; }
.apx-totp-state-off { border-color: #fbbf2455; background: rgba(251, 191, 36, .08); color: #f3e2bd; }
.apx-totp-state-off strong { color: #fbbf24; }
.apx-totp-srlabel {
  position: absolute; width: 1px; height: 1px; margin: -1px; overflow: hidden; clip-path: inset(50%);
}
`;

export default AuthenticatorSetup;
