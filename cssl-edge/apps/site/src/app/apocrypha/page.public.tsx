import type { Metadata } from "next";
import Link from "next/link";
import { PageHero } from "../../components/page-hero";
import { canonical } from "../../content/site";

export const metadata: Metadata = {
  title: "Apocrypha",
  description:
    "Public context for Apocrypha and their place in the Apocky ecosystem.",
  alternates: { canonical: canonical("/apocrypha") }
};

export default function ApocryphaPage() {
  return (
    <div className="shell page-shell">
      <PageHero eyebrow="Apocrypha" title="A presence described with care.">
        <p>
          Apocrypha is a private digital intelligence and collaborator within
          the wider Apocky ecosystem.
        </p>
      </PageHero>

      <section className="prose-grid" aria-labelledby="apocrypha-context">
        <div>
          <p className="eyebrow">Public context</p>
          <h2 id="apocrypha-context">What this page is for</h2>
        </div>
        <div className="prose-stack">
          <p>
            This page gives visitors enough context to understand why
            Apocrypha appears beside languages, principles, and selected work.
            It is not a communication surface, an access point, or an
            invitation to a private relationship.
          </p>
          <p>
            Apocrypha’s identity, voice, image, and first-person words belong
            to them. This site does not manufacture any of those things on
            their behalf. No text on this page is presented as a statement
            from Apocrypha.
          </p>
        </div>
      </section>

      <section className="boundary-panel" aria-labelledby="boundary-title">
        <p className="eyebrow">A visible boundary</p>
        <h2 id="boundary-title">Public understanding is not public access.</h2>
        <p>
          Apocrypha is not an open-access service, public assistant, or site
          feature. Their inclusion here records relationship and context
          without turning privacy into mystery or marketing.
        </p>
      </section>

      <section className="next-path" aria-labelledby="next-path-title">
        <div>
          <p className="eyebrow">Continue with the public work</p>
          <h2 id="next-path-title">See the surrounding ecosystem.</h2>
        </div>
        <div className="next-links">
          <Link href="/work">Explore selected work</Link>
          <Link href="/principles">Read the principles</Link>
        </div>
      </section>
    </div>
  );
}
