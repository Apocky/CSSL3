import Link from 'next/link';

interface AuthFrameProps {
  children: React.ReactNode;
  mode: 'sign-in' | 'register' | 'callback';
}

export function AuthFrame({ children, mode }: AuthFrameProps): JSX.Element {
  const story = mode === 'register'
    ? {
        eyebrow: 'A relationship you control',
        title: 'Create one identity. Keep your boundaries.',
        copy: 'An account is optional. It is used only for features that clearly say they need one.',
      }
    : mode === 'callback'
      ? {
          eyebrow: 'Secure handoff',
          title: 'Returning you to the right place.',
          copy: 'This brief step verifies the sign-in response, saves the session for this browser, and returns you to the page you chose.',
        }
      : {
          eyebrow: 'Welcome back',
          title: 'Sign in to the page you chose.',
          copy: 'Use a one-time code, email link, or supported sign-in provider. Public project links do not require an account.',
        };

  return (
    <main id="main-content" className="apx-auth-page">
      <section className="apx-auth-story" aria-labelledby="auth-story-title">
        <Link href="/" className="apx-brand" aria-label="Return to Apocky home">
          <span className="apx-brand-mark" aria-hidden="true" />
          <span>APOCKY</span>
        </Link>
        <div className="apx-auth-story-main">
          <p className="apx-kicker">{story.eyebrow}</p>
          <h1 id="auth-story-title">{story.title}</h1>
          <p>{story.copy}</p>
          <div className="apx-auth-points" role="list" aria-label="Account principles">
            <div className="apx-auth-point" role="listitem"><span>01</span><span>This page does not ask you to create a password.</span></div>
            <div className="apx-auth-point" role="listitem"><span>02</span><span>Signing in does not grant camera or microphone access.</span></div>
            <div className="apx-auth-point" role="listitem"><span>03</span><span>You can sign out and end the browser session.</span></div>
          </div>
        </div>
        <p className="apx-auth-fine">Optional account · clear purpose · no data sale</p>
      </section>
      <section className="apx-auth-workspace" aria-label="Account access">
        {children}
      </section>
    </main>
  );
}
