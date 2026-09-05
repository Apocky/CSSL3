import type { Metadata } from "next";
import { PageHero } from "../../components/page-hero";
import { canonical } from "../../content/site";

export const metadata: Metadata = {
  title: "Privacy",
  description: "How the public Apocky site handles visitor information.",
  alternates: { canonical: canonical("/privacy") }
};

export default function PrivacyPage() {
  return (
    <div className="shell page-shell information-page">
      <PageHero eyebrow="Privacy" title="A public site with a small data surface.">
        <p>
          This website is designed for reading and navigation. It does not
          provide public accounts, forms, chat, comments, or a site analytics
          prompt.
        </p>
      </PageHero>

      <div className="policy-stack">
        <section>
          <h2>Information the application does not request</h2>
          <p>
            The public application does not ask for a name, email address,
            profile, message, microphone, camera, payment information, or
            marketing preference. It does not set an application account
            cookie.
          </p>
        </section>
        <section>
          <h2>Basic delivery and security data</h2>
          <p>
            Like any website, hosting and network infrastructure receives
            request information needed to return a page and protect the
            service. This can include an IP address, browser information,
            requested URL, time, and security signals. The public application
            does not expose an interface for adding that information to a
            profile.
          </p>
        </section>
        <section>
          <h2>External destinations</h2>
          <p>
            Links labeled as external leave apocky.com. The destination’s own
            privacy practices apply after the visitor follows one of those
            links.
          </p>
        </section>
        <section>
          <h2>Scope</h2>
          <p>
            This notice describes the public website only. It does not describe
            separately protected, non-public systems.
          </p>
        </section>
      </div>
    </div>
  );
}
