import { useState } from 'react';
import type { Act } from '../App.tsx';
import { ipc } from '../lib/ipc.ts';
import type { View } from '../lib/view.ts';

interface Props {
  view: View;
  busy: boolean;
  act: Act;
}

export function SignIn({ view, busy, act }: Props) {
  const [email, setEmail] = useState(view.email_draft);
  const [code, setCode] = useState('');
  const [password, setPassword] = useState('');

  const address = email.trim();
  const ready = address.length > 0 && !busy;

  return (
    <main className="signin">
      <header className="signin-head">
        <p className="eyebrow">Apocrypha · Desktop</p>
        <h1>Sign in to Apocrypha.</h1>
        <p className="lead">
          Use your Apocky account. Your conversations are held by the service and stay private to your account.
        </p>
      </header>

      <section className="panel" aria-label="Sign in">
        <label htmlFor="email">Email address</label>
        <input
          id="email"
          type="email"
          autoComplete="email"
          spellCheck={false}
          value={email}
          disabled={busy}
          onChange={(event) => setEmail(event.target.value)}
          placeholder="you@example.com"
        />

        <div className="row">
          <button className="ghost" disabled={!ready} onClick={() => void act(() => ipc.sendCode(address))}>
            Email me a sign-in code
          </button>
          <button className="ghost" disabled={!ready} onClick={() => void act(() => ipc.createAccount(address))}>
            Create an account
          </button>
        </div>

        {view.code_sent ? (
          <>
            <label htmlFor="code">Code from your email</label>
            <div className="row">
              <input
                id="code"
                inputMode="numeric"
                autoComplete="one-time-code"
                value={code}
                disabled={busy}
                onChange={(event) => setCode(event.target.value)}
                placeholder="123456"
              />
              <button
                className="primary"
                disabled={!ready || code.trim().length < 6}
                onClick={() => void act(() => ipc.signIn(address, code.trim(), false))}
              >
                Verify code
              </button>
            </div>
          </>
        ) : null}

        <label htmlFor="password">Password</label>
        <div className="row">
          <input
            id="password"
            type="password"
            autoComplete="current-password"
            value={password}
            disabled={busy}
            onChange={(event) => setPassword(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && ready && password) void act(() => ipc.signIn(address, password, true));
            }}
            placeholder="Your account password"
          />
          <button
            className="primary"
            disabled={!ready || password.length < 1}
            onClick={() => void act(() => ipc.signIn(address, password, true))}
          >
            Sign in
          </button>
        </div>
        <button
          className="link"
          disabled={!ready || password.length < 8}
          onClick={() => void act(() => ipc.createWithPassword(address, password))}
        >
          Create an account with this password instead
        </button>
      </section>

      <p className="notice" role="status">
        {busy ? 'Working…' : view.notice}
      </p>

      {!view.configured && !busy ? (
        <button className="ghost" onClick={() => void act(() => ipc.bootstrap())}>
          Retry connection
        </button>
      ) : null}

      <footer className="signin-foot">
        <span>apocky.com</span>
        <span>Your message text is sent to the service to answer it. Nothing is stored on this computer except your
          sign-in and a list of your conversation identifiers.</span>
      </footer>
    </main>
  );
}
