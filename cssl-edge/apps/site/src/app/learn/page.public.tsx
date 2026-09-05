import type { Metadata } from "next";
import Link from "next/link";
import { ExternalLink } from "../../components/external-link";
import { PageHero } from "../../components/page-hero";
import { canonical } from "../../content/site";

export const metadata: Metadata = {
  title: "Learn",
  description:
    "A plain-language orientation to Apocky, Apocrypha, CSSL, and CSLv3.",
  alternates: { canonical: canonical("/learn") }
};

const glossary = [
  {
    term: "Apocky",
    definition:
      "The name connecting the creator, this public home, and the wider body of work presented here."
  },
  {
    term: "Apocrypha",
    definition:
      "A private digital intelligence and collaborator whose place in the ecosystem is described publicly without providing public access."
  },
  {
    term: "CSSL",
    definition:
      "A compiled programming language, runtime, standard library, and substrate."
  },
  {
    term: "CSLv3",
    definition:
      "The third generation of Caveman Spec Language: a dense specification notation for collaboration, reasoning, and compiler input."
  }
] as const;

export default function LearnPage() {
  return (
    <div className="shell page-shell">
      <PageHero eyebrow="Learn" title="Start with the distinctions.">
        <p>
          The ecosystem makes more sense when its names stay precise. This
          page is a short orientation, not a technical manual.
        </p>
      </PageHero>

      <section className="glossary" aria-labelledby="glossary-title">
        <div className="section-heading compact">
          <p className="eyebrow">Four useful bearings</p>
          <h2 id="glossary-title">A plain-language map</h2>
        </div>
        <dl>
          {glossary.map((item, index) => (
            <div className="glossary-entry" key={item.term}>
              <dt>
                <span aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                {item.term}
              </dt>
              <dd>{item.definition}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section className="learn-pair" aria-labelledby="learn-pair-title">
        <div className="section-heading">
          <p className="eyebrow">Related, not interchangeable</p>
          <h2 id="learn-pair-title">Two language projects</h2>
        </div>
        <div className="comparison-grid">
          <article>
            <p className="comparison-type">Compiled language</p>
            <h3>CSSL</h3>
            <p>
              CSSL concerns executable programs, compilation, runtime
              behavior, and a broader substrate.
            </p>
            <ExternalLink href="https://cssl.dev">Visit CSSL</ExternalLink>
          </article>
          <article>
            <p className="comparison-type">Specification notation</p>
            <h3>CSLv3</h3>
            <p>
              CSLv3 compresses relationships, constraints, evidence, and
              decisions into a dense notation.
            </p>
            <ExternalLink href="https://cssl.dev/CSLv3">Visit CSLv3</ExternalLink>
          </article>
        </div>
      </section>

      <section className="next-path" aria-labelledby="learn-next-title">
        <div>
          <p className="eyebrow">From orientation to practice</p>
          <h2 id="learn-next-title">See what guides the work.</h2>
        </div>
        <div className="next-links">
          <Link href="/principles">Read the principles</Link>
          <Link href="/work">Explore selected work</Link>
        </div>
      </section>
    </div>
  );
}
