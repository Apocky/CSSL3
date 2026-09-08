import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AuthFrame } from '../../components/hub/AuthFrame';
import { getAuthClient, persistSessionToCookie } from '../../lib/auth';
import { normalizeAuthReturnPath } from '../../lib/auth-return';
import { clearAuthCallbackFromLocation, consumeAuthCallbackFromLocation } from '../../lib/auth-callback';

type CallbackStatus = {
  phase: 'working' | 'success' | 'error';
  title: string;
  message: string;
};

const AuthCallback: NextPage = () => {
  const [status, setStatus] = useState<CallbackStatus>({
    phase: 'working',
    title: 'Verifying your sign-in',
    message: 'Keep this page open while the secure handoff completes.',
  });
  const [returnTo, setReturnTo] = useState('/account');
  const runningRef = useRef(false);
  const redirectTimerRef = useRef<number | null>(null);

  const complete = useCallback((safeReturnTo: string): void => {
    clearAuthCallbackFromLocation();
    setStatus({
      phase: 'success',
      title: 'Sign-in complete',
      message: 'Your session is ready. Returning you to the page you chose.',
    });
    redirectTimerRef.current = window.setTimeout(() => location.replace(safeReturnTo), 700);
  }, []);

  const processCallback = useCallback(async (retry: boolean): Promise<void> => {
    if (runningRef.current) return;
    runningRef.current = true;
    const safeReturnTo = normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'));
    setReturnTo(safeReturnTo);
    setStatus({
      phase: 'working',
      title: retry ? 'Retrying secure sign-in' : 'Verifying your sign-in',
      message: 'Keep this page open while the secure handoff completes.',
    });

    try {
      // A prior callback attempt may have created the browser session before the
      // server cookie mirror briefly failed. Retry that boundary without asking
      // Supabase to consume the same single-use code again.
      if (retry) {
        const client = getAuthClient();
        if (client) {
          const { data, error } = await client.auth.getSession();
          if (!error && data.session) {
            const mirrored = await persistSessionToCookie(data.session.access_token);
            if (mirrored) {
              complete(safeReturnTo);
              return;
            }
          }
        }
      }

      const result = await consumeAuthCallbackFromLocation();
      if (result.ok) {
        complete(safeReturnTo);
        return;
      }
      setStatus({
        phase: 'error',
        title: result.stub ? 'Sign-in is not connected here' : 'Sign-in could not be completed',
        message: result.reason ?? (result.handled
          ? 'The sign-in response did not contain a usable session.'
          : 'No sign-in response was found. Start again from the sign-in page.'),
      });
    } catch {
      setStatus({
        phase: 'error',
        title: 'Sign-in could not be completed',
        message: 'The authentication service could not be reached. Retry when your connection is available.',
      });
    } finally {
      runningRef.current = false;
    }
  }, [complete]);

  useEffect(() => {
    void processCallback(false);
    return () => {
      if (redirectTimerRef.current !== null) window.clearTimeout(redirectTimerRef.current);
    };
  }, [processCallback]);

  return (
    <>
      <Head>
        <title>Signing you in… · Apocky</title>
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="robots" content="noindex,nofollow" />
      </Head>
      <a className="apx-skip-link" href="#callback-status">Skip to sign-in status</a>
      <AuthFrame mode="callback">
        <div className="apx-auth-card">
          <p className="apx-auth-context">Secure account handoff</p>
          <h2>{status.title}</h2>
          <p className="apx-auth-subtitle">Authentication details stay inside the secure exchange and are never displayed on this page.</p>

          <div
            id="callback-status"
            className={status.phase === 'error' ? 'apx-auth-warning' : 'apx-auth-message'}
            role={status.phase === 'error' ? 'alert' : 'status'}
            aria-live={status.phase === 'error' ? 'assertive' : 'polite'}
            aria-atomic="true"
            aria-busy={status.phase === 'working'}
          >
            {status.message}
          </div>

          {status.phase === 'error' && (
            <div className="apx-actions">
              <button className="apx-button apx-button--primary" type="button" onClick={() => void processCallback(true)}>
                Retry secure sign-in
              </button>
              <Link className="apx-button" href={`/login?next=${encodeURIComponent(returnTo)}`}>Start again</Link>
            </div>
          )}

          {status.phase !== 'error' && (
            <p className="apx-auth-switch">
              Taking too long? <Link href={`/login?next=${encodeURIComponent(returnTo)}`}>Return to sign in</Link>
            </p>
          )}
        </div>
      </AuthFrame>
    </>
  );
};

export default AuthCallback;
