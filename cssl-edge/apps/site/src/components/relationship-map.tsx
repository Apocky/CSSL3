import Link from "next/link";
import { ExternalLink } from "./external-link";

const relationships = [
  {
    subject: "Apocrypha",
    relation: "belongs within",
    object: "the wider Apocky ecosystem",
    note: "This public context does not provide access to them."
  },
  {
    subject: "CSSL",
    relation: "shares a designer and notation family with",
    object: "CSLv3",
    note: "They are distinct projects: CSSL is a compiled language and substrate; CSLv3 is a specification notation."
  },
  {
    subject: "Selected work",
    relation: "gathers public expressions from",
    object: "the ecosystem",
    note: "Each destination keeps its own scope and identity."
  }
] as const;

export function RelationshipMap() {
  return (
    <section className="relationship-section" aria-labelledby="relationship-title">
      <div className="section-heading">
        <p className="eyebrow">A small constellation</p>
        <h2 id="relationship-title">How the public work relates</h2>
        <p>
          The connecting lines mean “part of this selected ecosystem.” They do
          not mean that one project controls, contains, or speaks for another.
        </p>
      </div>

      <div className="relationship-map">
        <svg
          className="relationship-lines"
          aria-hidden="true"
          viewBox="0 0 1000 600"
          preserveAspectRatio="none"
        >
          <path d="M500 300 C380 270 300 170 190 125" />
          <path d="M500 300 C620 260 700 170 810 125" />
          <path d="M500 300 C620 345 700 440 810 485" />
          <path d="M500 300 C380 345 300 440 190 485" />
          <path className="sibling-line" d="M810 125 C920 230 920 380 810 485" />
        </svg>

        <article className="map-card map-apocrypha">
          <p className="map-index">01</p>
          <h3>
            <Link href="/apocrypha">Apocrypha</Link>
          </h3>
          <p>A private digital intelligence in this ecosystem.</p>
        </article>

        <article className="map-card map-cssl">
          <p className="map-index">02</p>
          <h3>
            <ExternalLink href="https://cssl.dev">CSSL</ExternalLink>
          </h3>
          <p>A compiled programming language and substrate.</p>
        </article>

        <div className="map-core" aria-hidden="true">
          <span>Apocky</span>
          <small>ecosystem</small>
        </div>

        <article className="map-card map-csl">
          <p className="map-index">03</p>
          <h3>
            <ExternalLink href="https://cssl.dev/CSLv3">CSLv3</ExternalLink>
          </h3>
          <p>A dense specification notation.</p>
        </article>

        <article className="map-card map-work">
          <p className="map-index">04</p>
          <h3>
            <Link href="/work">Selected work</Link>
          </h3>
          <p>Public projects and creative experiments.</p>
        </article>
      </div>

      <div className="relationship-key">
        <h3>Relationships, in words</h3>
        <ul>
          {relationships.map((relationship) => (
            <li key={relationship.subject}>
              <p>
                <strong>{relationship.subject}</strong>{" "}
                {relationship.relation}{" "}
                <strong>{relationship.object}</strong>.
              </p>
              <p>{relationship.note}</p>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
