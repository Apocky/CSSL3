import Link from "next/link";

export default function NotFound() {
  return (
    <div className="shell not-found">
      <p className="eyebrow">404 / not found</p>
      <h1>This path is not part of the public site.</h1>
      <p>
        The address may be old or incomplete. Choose one of the public paths
        below.
      </p>
      <nav aria-label="Return to the public site">
        <Link className="button-link" href="/">
          Return home <span aria-hidden="true">→</span>
        </Link>
        <Link className="text-link" href="/work">
          Explore the work
        </Link>
      </nav>
    </div>
  );
}
