import type { AnchorHTMLAttributes, ReactNode } from "react";

type ExternalLinkProps = Omit<
  AnchorHTMLAttributes<HTMLAnchorElement>,
  "href" | "children"
> & {
  href: `https://${string}`;
  children: ReactNode;
};

export function ExternalLink({
  children,
  className,
  ...props
}: ExternalLinkProps) {
  return (
    <a
      {...props}
      className={className}
      rel="external"
    >
      <span>{children}</span>
      <span className="external-mark" aria-hidden="true">
        ↗
      </span>
      <span className="sr-only">, external site</span>
    </a>
  );
}
