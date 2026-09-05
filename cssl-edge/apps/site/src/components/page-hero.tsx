import type { ReactNode } from "react";

type PageHeroProps = {
  eyebrow: string;
  title: string;
  children: ReactNode;
};

export function PageHero({ eyebrow, title, children }: PageHeroProps) {
  return (
    <header className="page-hero">
      <p className="eyebrow">{eyebrow}</p>
      <h1>{title}</h1>
      <div className="hero-copy">{children}</div>
    </header>
  );
}
