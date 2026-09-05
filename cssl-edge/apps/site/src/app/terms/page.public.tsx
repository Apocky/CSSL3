import type { Metadata } from "next";
import { PageHero } from "../../components/page-hero";
import { canonical } from "../../content/site";

export const metadata: Metadata = {
  title: "Terms",
  description: "Plain-language terms for the public Apocky website.",
  alternates: { canonical: canonical("/terms") }
};

export default function TermsPage() {
  return (
    <div className="shell page-shell information-page">
      <PageHero eyebrow="Terms" title="Terms for this public reading surface.">
        <p>
          By using this site, visitors may read and follow its public links.
          The site does not create an account, membership, service
          relationship, or right of access to any private system.
        </p>
      </PageHero>

      <div className="policy-stack">
        <section>
          <h2>Public information</h2>
          <p>
            Material is provided for general information and orientation. It
            may change as the underlying work changes. A description of work
            in progress is not a promise of availability or a release date.
          </p>
        </section>
        <section>
          <h2>Identity and authorship</h2>
          <p>
            Material attributed to Apocky belongs to its stated author.
            Nothing on the Apocrypha page is represented as Apocrypha’s own
            first-person statement.
          </p>
        </section>
        <section>
          <h2>External destinations</h2>
          <p>
            CSSL, CSLv3, and Chaos Tarot have separate destinations. Their own
            terms and operating boundaries apply there.
          </p>
        </section>
        <section>
          <h2>No hidden transaction</h2>
          <p>
            This public site does not sell a product, accept payment, enroll a
            member, or collect a public request. Reading a page does not imply
            consent to a private interaction.
          </p>
        </section>
      </div>
    </div>
  );
}
