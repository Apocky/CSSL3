import type { Metadata, Viewport } from "next";
import "@apocky/visual-tokens/styles.css";
import "./globals.css";
import { SiteFooter } from "../components/site-footer";
import { SiteHeader } from "../components/site-header";
import { siteIdentity } from "../content/site";

export const dynamic = "error";

export const metadata: Metadata = {
  metadataBase: new URL(siteIdentity.url),
  title: {
    default: "Apocky — a living body of work",
    template: "%s — Apocky"
  },
  description: siteIdentity.description,
  applicationName: siteIdentity.name,
  alternates: {
    canonical: "/"
  },
  robots: {
    index: true,
    follow: true
  },
  openGraph: {
    type: "website",
    url: siteIdentity.url,
    siteName: siteIdentity.name,
    title: "Apocky — a living body of work",
    description: siteIdentity.description,
    locale: "en_US"
  },
  twitter: {
    card: "summary",
    title: "Apocky — a living body of work",
    description: siteIdentity.description
  }
};

export const viewport: Viewport = {
  colorScheme: "light dark",
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f7f0e5" },
    { media: "(prefers-color-scheme: dark)", color: "#1d1b19" }
  ],
  width: "device-width",
  initialScale: 1
};

export default function RootLayout({
  children
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>
        <SiteHeader />
        <main id="main-content">{children}</main>
        <SiteFooter />
      </body>
    </html>
  );
}
