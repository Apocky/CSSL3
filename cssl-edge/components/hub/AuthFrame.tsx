import Link from 'next/link';
import styles from '../../styles/AuthEntry.module.css';

interface AuthFrameProps {
  children: React.ReactNode;
  mode: 'sign-in' | 'register' | 'callback' | 'setup';
  formFirst?: boolean;
}

export function AuthFrame({ children, mode, formFirst = false }: AuthFrameProps): JSX.Element {
  // 'setup' is the closing step of a sign-in that has ALREADY succeeded: the reader is signed in
  // and is creating an authenticator. Greeting them with "Welcome back / sign in with a 6-digit
  // code from your authenticator app" described the credential they were on that screen to make.
  // The enrolment card owns the headline there, so this panel keeps only the brand and the
  // assurances — a competing headline is the exact problem AuthEntry.module.css records.
  const story = mode === 'setup'
    ? { eyebrow: '', title: '', copy: '' }
    : mode === 'register'
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
          // The form beside this one says "No password, no email round-trip." Saying "email link"
          // here contradicted it on the same screen, and the email path is the FALLBACK now, not
          // the headline. Describing the primary way in is what this panel is for.
          copy: 'Sign in with a 6-digit code from your authenticator app, or with a supported provider. Reading Apocrypha and the public pages needs no account at all.',
        };

  // aria-labelledby only when the heading it names actually renders — in 'setup' it does not, and
  // a labelledby pointing at a missing id leaves the region with NO accessible name at all, which
  // is worse than the static label below.
  const storySection = <section
    className="apx-auth-story"
    {...(story.title ? { 'aria-labelledby': 'auth-story-title' } : { 'aria-label': 'About your Apocky account' })}
  >
        <Link href="/" className="apx-brand" aria-label="Return to Apocky home">
          <span className="apx-brand-mark" aria-hidden="true" />
          <span>APOCKY</span>
        </Link>
        <div className="apx-auth-story-main">
          {story.eyebrow ? <p className="apx-kicker">{story.eyebrow}</p> : null}
          {story.title
            ? (formFirst ? <h2 id="auth-story-title">{story.title}</h2> : <h1 id="auth-story-title">{story.title}</h1>)
            : null}
          {story.copy ? <p>{story.copy}</p> : null}
          <div className="apx-auth-points" role="list" aria-label="Account principles">
            <div className="apx-auth-point" role="listitem"><span>01</span><span>This page does not ask you to create a password.</span></div>
            <div className="apx-auth-point" role="listitem"><span>02</span><span>Signing in does not grant camera or microphone access.</span></div>
            <div className="apx-auth-point" role="listitem"><span>03</span><span>You can sign out and end the browser session.</span></div>
          </div>
        </div>
        <p className="apx-auth-fine">Optional account · clear purpose · no data sale</p>
      </section>;
  const workspace = <section className="apx-auth-workspace" aria-label="Account access">
    {formFirst ? <div className={styles.formWrap}><Link className={styles.returnHome} href="/">← Home</Link>{children}</div> : children}
  </section>;
  return (
    <main id="main-content" className={`apx-auth-page${formFirst ? ` ${styles.formFirst}` : ''}`}>
      {formFirst ? <>{workspace}{storySection}</> : <>{storySection}{workspace}</>}
    </main>
  );
}
