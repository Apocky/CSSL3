import Link from "next/link";
import { primaryNavigation } from "../content/site";

export function SiteHeader() {
  return (
    <header className="site-header">
      <a className="skip-link" href="#main-content">
        Skip to content
      </a>
      <div className="shell header-inner">
        <Link className="wordmark" href="/" aria-label="Apocky home">
          <span className="wordmark-seed" aria-hidden="true" />
          <span>apocky</span>
        </Link>
        <nav aria-label="Primary navigation">
          <ul className="primary-nav">
            {primaryNavigation.map((item) => (
              <li key={item.href}>
                <Link href={item.href}>{item.label}</Link>
              </li>
            ))}
          </ul>
        </nav>
      </div>
    </header>
  );
}
