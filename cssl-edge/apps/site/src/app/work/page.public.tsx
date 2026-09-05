import type { Metadata } from "next";
import Link from "next/link";
import { ExternalLink } from "../../components/external-link";
import { PageHero } from "../../components/page-hero";
import { canonical, externalDestinations } from "../../content/site";

export const metadata: Metadata = {
  title: "Work",
  description:
    "A selected public view of CSSL, CSLv3, Chaos Tarot, and the ideas connecting them.",
  alternates: { canonical: canonical("/work") }
};

export default function WorkPage() {
  return (
    <div className="shell page-shell">
      <PageHero eyebrow="Selected work" title="Different forms. A shared insistence on coherence.">
        <p>
          This is a deliberately small public selection. Each work keeps its
          own purpose, vocabulary, and destination.
        </p>
      </PageHero>

      <section className="work-list" aria-label="Selected projects">
        {externalDestinations.map((destination, index) => (
          <article className="work-entry" key={destination.href}>
            <p className="work-number" aria-hidden="true">
              {String(index + 1).padStart(2, "0")}
            </p>
            <div>
              <p className="eyebrow">
                {destination.label === "Chaos Tarot" ? "Creative work" : "Language and systems"}
              </p>
              <h2>{destination.label}</h2>
              <p>{destination.description}</p>
            </div>
            <ExternalLink className="project-link" href={destination.href}>
              Visit {destination.label}
            </ExternalLink>
          </article>
        ))}
      </section>

      <section className="distinction-note" aria-labelledby="distinction-title">
        <div>
          <p className="eyebrow">Keep the distinction</p>
          <h2 id="distinction-title">CSSL is not CSLv3.</h2>
        </div>
        <div>
          <p>
            CSSL is a compiled programming language and substrate. CSLv3 is a
            specification notation. They share a designer and a notation
            family, but they are separate projects.
          </p>
          <Link className="text-link with-arrow" href="/learn">
            Learn the landscape <span aria-hidden="true">→</span>
          </Link>
        </div>
      </section>
    </div>
  );
}
