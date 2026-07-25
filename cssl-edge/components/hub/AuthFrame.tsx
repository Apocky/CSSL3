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
        copy: 'Your Apocky account connects the work without turning you into a product. Capabilities remain explicit, revocable, and visible.',
      }
    : mode === 'callback'
      ? {
          eyebrow: 'Secure handoff',
          title: 'Returning you to the right place.',
          copy: 'This brief step verifies the sign-in response, stores the session safely, and returns you to the path you chose.',
        }
      : {
          eyebrow: 'Welcome back',
          title: 'Continue the conversation without losing the thread.',
          copy: 'One account reconnects your session across Apocky while keeping private interaction behind an explicit doorway.',
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
            <div className="apx-auth-point" role="listitem"><span>01</span><span>Passwordless by default; no password database to breach.</span></div>
            <div className="apx-auth-point" role="listitem"><span>02</span><span>Your account does not grant hidden camera, memory, or effect permissions.</span></div>
            <div className="apx-auth-point" role="listitem"><span>03</span><span>Sessions and connected capabilities can be revoked.</span></div>
          </div>
        </div>
        <p className="apx-auth-fine">sovereignty-respecting · no data sale · explicit capability boundaries</p>
      </section>
      <section className="apx-auth-workspace" aria-label="Account access">
        {children}
      </section>
    </main>
  );
}

