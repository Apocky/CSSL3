# Codex Apockalypsis

The public reading edition of **The Good Book**, available at [apocky.com/codex-apockalypsis](https://www.apocky.com/codex-apockalypsis).

The current collection contains one novel opening and its paired Codex, three voice treatments, a twelve-volume plan with 205 chapter designs, the reference collection, three atlas maps, and 260 source catalogue records. The full series remains in progress.

## Files and editing

The website lives in `../../public/codex-apockalypsis/`. Its reviewed `content.json` is the source for the landing page and 19 reader pages. The same directory holds the reviewed Markdown files, maps, PDF/Word reading editions, and public archive. These source assets are preserved when rebuilding; this renderer does not recreate the PDF, Word, or ZIP editions.

Edit the corresponding document bodies and metadata in `content.json`, then update their Markdown downloads and any affected reading editions or archive. Keep the novel and paired Codex synchronized. Maintain historical citations, work-in-progress descriptions, and spoiler notices. Original manuscripts, raw conversations, private voice material, local source inventories, and deployment credentials are outside this publication package.

`build.mjs` renders the HTML with the pinned `marked` dependency. `codex.css` and `codex.js` are the editable browser assets; rebuilding copies them into the public assets directory. Raw HTML in Markdown is escaped, and rendered links accept only HTTP(S), site-relative, or fragment destinations.

## Rebuild and verify

Use Node.js 20 or newer. From this directory:

```sh
npm ci --ignore-scripts
npm run verify
```

Verification builds into a new temporary directory and compares every public file by SHA-256, then removes only that temporary directory. A successful verification leaves the checked-in publication unchanged.

After an intentional content or styling edit, rebuild the public pages:

```sh
npm run build
npm run verify
```

To inspect an independent build, choose an empty output directory:

```sh
node build.mjs --out ./preview
```

The publication requires two explicit Next.js rewrites: `/codex-apockalypsis` to its `index.html`, and `/codex-apockalypsis/library/:slug` to each reader's `index.html`. Its homepage card belongs to `MORE_PATHS` in `pages/index.tsx`. No catchall route or application dependency change is needed.

## Publication and repository history

The initial live publication was an additive overlay onto the verified production deployment. This Git change carries the same 69 public files and the same two route additions while retaining newer homepage and application work already on its Git parent. The entire Git checkout is therefore not a byte-for-byte reconstruction of that historical live deployment.

Review browser reading behavior, downloads, citations, links, responsive layouts, and the publication's actual scope before publishing updates. Use the current production source and preserve unrelated live changes. A repository push or local build does not establish that the public domain serves a new edition.
