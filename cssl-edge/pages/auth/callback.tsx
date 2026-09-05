import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';
import { AuthFrame } from '../../components/hub/AuthFrame';
import {
  closeAuthenticationAfterPrivateLockFailure,
  closeAuthenticationAttemptAfterPrivateLockFailure,
  getAuthClient,
  persistSessionToCookie,
  type AuthSessionMirrorResult,
} from '../../lib/auth';
import { normalizeAuthReturnPath } from '../../lib/auth-return';
import { clearAuthCallbackFromLocation, consumeAuthCallbackFromLocation } from '../../lib/auth-callback';
import { lockMiniBrainForSignedOutSession, stageMiniBrainOwnerRebindAfterReauthentication } from '../../lib/brain/mini-brain';

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
  const freshCallbackSessionRef = useRef<{
    accessToken: string;
    subjectKey: string;
    authAttempt: string;
  } | null>(null);

  const protectMirrorFailure = useCallback(async (mirrorStatus: AuthSessionMirrorResult['status']): Promise<void> => {
    const lock = await lockMiniBrainForSignedOutSession();
    if (lock.status === 'durability_unconfirmed') {
      await closeAuthenticationAfterPrivateLockFailure();
      freshCallbackSessionRef.current = null;
      clearAuthCallbackFromLocation();
      setStatus({
        phase: 'error',
        title: 'Authentication boundary not verified',
        message: 'The provider verified your identity, but neither the server-session commit nor a durable private Brain lock could be confirmed. Authentication was closed where possible. Enable browser storage or clear this site\'s data, then start sign-in again. (AUTH_SESSION_COMMIT_AND_BRAIN_LOCK_UNCONFIRMED · MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED)',
      });
      return;
    }
    setStatus({
      phase: 'error',
      title: 'Secure handoff needs a retry',
      message: mirrorStatus === 'commit_uncertain'
        ? 'The server may have committed your session, but the browser could not verify the final handoff. The private Brain was locked. Retry this handoff without restarting the provider sign-in. (AUTH_SESSION_COMMIT_UNCERTAIN)'
        : 'The provider session is verified, but the secure site session was not established. The private Brain was locked. Retry this handoff.',
    });
  }, []);

  const closeUncertainProviderSession = useCallback(async (authAttempt: string): Promise<void> => {
    const closure = await closeAuthenticationAttemptAfterPrivateLockFailure(
      authAttempt,
      lockMiniBrainForSignedOutSession,
    );
    if (closure.status === 'superseded') {
      freshCallbackSessionRef.current = null;
      clearAuthCallbackFromLocation();
      setStatus({
        phase: 'success',
        title: 'A newer sign-in is already active',
        message: 'This older provider handoff was superseded by a newer secure sign-in. No session or private Brain state was changed. You can close this page. (AUTH_CALLBACK_SUPERSEDED)',
      });
      return;
    }
    const lock = closure.beforeCloseResult;
    freshCallbackSessionRef.current = null;
    clearAuthCallbackFromLocation();
    setStatus(lock?.status === 'durability_unconfirmed'
      ? {
          phase: 'error',
          title: 'Authentication boundary not verified',
          message: 'The provider-session result and durable private Brain lock are both uncertain. Authentication was closed where possible. Clear this site\'s browser data, then start sign-in again. (AUTH_PROVIDER_SESSION_AND_BRAIN_LOCK_UNCONFIRMED · MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED)',
        }
      : {
          phase: 'error',
          title: 'Provider handoff did not settle',
          message: 'The provider-session result could not be proven before the deadline. The private Brain was locked and authentication was closed where possible. Start sign-in again. (AUTH_PROVIDER_SESSION_UNCERTAIN)',
        });
  }, []);

  const complete = useCallback(async (safeReturnTo: string): Promise<void> => {
    if (freshCallbackSessionRef.current) {
      const brainStage = await stageMiniBrainOwnerRebindAfterReauthentication(
        freshCallbackSessionRef.current.subjectKey,
        freshCallbackSessionRef.current.authAttempt,
      );
      freshCallbackSessionRef.current = null;
      if (brainStage.status === 'durability_unconfirmed') {
        await closeAuthenticationAfterPrivateLockFailure();
        clearAuthCallbackFromLocation();
        setStatus({
          phase: 'error',
          title: 'Private Brain lock not verified',
          message: 'Your identity was verified, but this browser could not confirm a durable private Brain lock. The new site session was closed where possible. Enable browser storage or clear this site\'s data, then start sign-in again. (MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED)',
        });
        return;
      }
    }
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
      if (retry && freshCallbackSessionRef.current) {
        const expected = freshCallbackSessionRef.current;
        const client = getAuthClient();
        if (client) {
          const { data, error } = await client.auth.getSession();
          if (
            !error
            && data.session
            && data.session.access_token === expected.accessToken
            && data.session.user.id === expected.subjectKey
          ) {
            const mirrored = await persistSessionToCookie(data.session.access_token, {
              reauthenticated: true,
              authAttempt: expected.authAttempt,
            });
            if (mirrored.status === 'established') {
              await complete(safeReturnTo);
              return;
            }
            await protectMirrorFailure(mirrored.status);
            return;
          }
        }
        const expectedAttempt = freshCallbackSessionRef.current?.authAttempt;
        if (expectedAttempt) await closeUncertainProviderSession(expectedAttempt);
        else await closeAuthenticationAfterPrivateLockFailure();
        return;
      }

      const result = await consumeAuthCallbackFromLocation();
      freshCallbackSessionRef.current ??= result.freshSession ?? null;
      if (result.ok) {
        await complete(safeReturnTo);
        return;
      }
      if (result.freshSession && result.mirrorStatus) {
        await protectMirrorFailure(result.mirrorStatus);
        return;
      }
      if (result.providerSessionUncertain && result.providerSessionAuthAttempt) {
        await closeUncertainProviderSession(result.providerSessionAuthAttempt);
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
  }, [closeUncertainProviderSession, complete, protectMirrorFailure]);

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
