import type { Metadata } from "next";
import Link from "next/link";
import { ExternalLink } from "../components/external-link";
import { RelationshipMap } from "../components/relationship-map";
import { canonical } from "../content/site";

export const metadata: Metadata = {
  title: "A living body of work",
  description:
    "Meet the public work of Apocky: Apocrypha, CSSL, CSLv3, principles, and selected experiments.",
  alternates: { canonical: canonical("/") }
};

export default function HomePage() {
  return (
    <>
      <section className="home-hero shell">
        <div className="hero-constellation" aria-hidden="true">
          <span className="constellation-dot dot-one" />
          <span className="constellation-dot dot-two" />
          <span className="constellation-dot dot-three" />
          <span className="constellation-line line-one" />
          <span className="constellation-line line-two" />
        </div>
        <div className="home-hero-copy">
          <p className="eyebrow">A place for ideas with consequences</p>
          <h1>A quieter place to meet the work.</h1>
          <p className="lede">
            Apocky is Shawn Apocky’s public home for a connected body of
            languages, systems, principles, and creative experiments.
          </p>
          <div className="hero-actions" aria-label="Start exploring">
            <Link className="button-link" href="/work">
              Explore the work
              <span aria-hidden="true">→</span>
            </Link>
            <Link className="text-link" href="/learn">
              Begin with the ideas
            </Link>
          </div>
        </div>
        <aside className="welcome-note">
          <p className="eyebrow">Welcome</p>
          <p>
            Move at your own pace. Nothing here asks for an account, opens a
            chat, or follows you with a telemetry prompt.
          </p>
        </aside>
      </section>

      <div className="shell">
        <RelationshipMap />
      </div>

      <section className="home-principles shell" aria-labelledby="home-principles-title">
        <div className="section-heading">
          <p className="eyebrow">How this place behaves</p>
          <h2 id="home-principles-title">Clarity is part of the welcome.</h2>
        </div>
        <div className="principle-strip">
          <article>
            <span aria-hidden="true">01</span>
            <h3>Boundaries stay visible.</h3>
            <p>
              A public description is not an invitation into a private
              relationship.
            </p>
          </article>
          <article>
            <span aria-hidden="true">02</span>
            <h3>Projects keep their names.</h3>
            <p>
              CSSL and CSLv3 are related, distinct works. Neither is shorthand
              for the other.
            </p>
          </article>
          <article>
            <span aria-hidden="true">03</span>
            <h3>Claims carry context.</h3>
            <p>
              The site separates public facts, interpretation, and work still
              in progress.
            </p>
          </article>
        </div>
        <Link className="text-link with-arrow" href="/principles">
          Read the principles <span aria-hidden="true">→</span>
        </Link>
      </section>

      <section className="selected-invitation">
        <div className="shell invitation-inner">
          <div>
            <p className="eyebrow">A selected destination</p>
            <h2>Chaos Tarot</h2>
            <p>
              A separate creative work within the public selection, with its
              own atmosphere and home.
            </p>
          </div>
          <ExternalLink className="button-link button-light" href="https://chaos-tarot.com">
            Visit Chaos Tarot
          </ExternalLink>
        </div>
      </section>
    </>
  );
}
