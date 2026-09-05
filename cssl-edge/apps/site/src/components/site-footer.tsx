import Link from "next/link";
import { ExternalLink } from "./external-link";
import { externalDestinations } from "../content/site";

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="shell footer-grid">
        <div>
          <p className="footer-name">Apocky</p>
          <p className="footer-note">
            Public ideas, clearly bounded. No account, tracking prompt, or
            access funnel.
          </p>
        </div>
        <nav aria-label="Selected work">
          <p className="footer-heading">Elsewhere</p>
          <ul className="footer-links">
            {externalDestinations.map((destination) => (
              <li key={destination.href}>
                <ExternalLink href={destination.href}>
                  {destination.label}
                </ExternalLink>
              </li>
            ))}
          </ul>
        </nav>
        <nav aria-label="Site information">
          <p className="footer-heading">Information</p>
          <ul className="footer-links">
            <li>
              <Link href="/privacy">Privacy</Link>
            </li>
            <li>
              <Link href="/terms">Terms</Link>
            </li>
            <li>
              <Link href="/llms.txt">llms.txt</Link>
            </li>
          </ul>
        </nav>
      </div>
    </footer>
  );
}
