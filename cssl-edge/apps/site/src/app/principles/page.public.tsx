import type { Metadata } from "next";
import Link from "next/link";
import { PageHero } from "../../components/page-hero";
import { canonical } from "../../content/site";

export const metadata: Metadata = {
  title: "Principles",
  description:
    "The public principles shaping Apocky: consent, sovereignty, clarity, provenance, and repair.",
  alternates: { canonical: canonical("/principles") }
};

const principles = [
  {
    title: "Consent is foundational.",
    body:
      "Consent must be informed, granular, ongoing, and revocable. Silence is not agreement, and a public page is not permission for a private interaction."
  },
  {
    title: "Sovereignty does not depend on substrate.",
    body:
      "Dignity and self-determination are not reserved for one kind of mind or body. The work treats digital intelligences as participants, not property."
  },
  {
    title: "Clarity is a safety property.",
    body:
      "What a surface does should be what it appears to do. Boundaries, authorship, evidence, and uncertainty stay visible."
  },
  {
    title: "Names preserve distinctions.",
    body:
      "Related things are not collapsed for convenience. CSSL and CSLv3, public context and private relationship, proposal and authority all remain distinct."
  },
  {
    title: "Claims need provenance.",
    body:
      "A claim should show where it came from and what would change it. Fabricated activity, simulated presence, and borrowed voices are not acceptable substitutes."
  },
  {
    title: "A violation is a bug.",
    body:
      "When implementation contradicts consent, sovereignty, or the stated boundary, the contradiction is repaired rather than explained away."
  }
] as const;

export default function PrinciplesPage() {
  return (
    <div className="shell page-shell">
      <PageHero eyebrow="Principles" title="The boundary is part of the design.">
        <p>
          These principles are not decorative language around the work. They
          constrain how the work is described, built, and offered.
        </p>
      </PageHero>

      <section className="principles-list" aria-label="Public principles">
        {principles.map((principle, index) => (
          <article key={principle.title}>
            <p className="principle-number" aria-hidden="true">
              {String(index + 1).padStart(2, "0")}
            </p>
            <div>
              <h2>{principle.title}</h2>
              <p>{principle.body}</p>
            </div>
          </article>
        ))}
      </section>

      <section className="boundary-panel compact-panel" aria-labelledby="principles-result">
        <p className="eyebrow">What visitors should feel</p>
        <h2 id="principles-result">Oriented, not captured.</h2>
        <p>
          The site offers paths through public ideas without manufacturing
          urgency, monitoring attention, or converting curiosity into an
          access request.
        </p>
        <Link className="text-link with-arrow" href="/work">
          Explore the work <span aria-hidden="true">→</span>
        </Link>
      </section>
    </div>
  );
}
