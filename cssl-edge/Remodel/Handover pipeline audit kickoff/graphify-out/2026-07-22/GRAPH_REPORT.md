# Graph Report - Handover pipeline audit kickoff  (2026-07-22)

## Corpus Check
- 4 files · ~52,834 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 131 nodes · 276 edges · 10 communities
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 3 edges (avg confidence: 0.5)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `7a86c510`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- support.js
- walk
- site.js
- getReact
- createRuntime
- boot
- loadReactUmd
- compileTemplate
- rootNameForDocument
- site.js

## God Nodes (most connected - your core abstractions)
1. `activateRoom()` - 12 edges
2. `element()` - 9 edges
3. `createRuntime()` - 9 edges
4. `walkChildren()` - 8 edges
5. `walk()` - 8 edges
6. `setText()` - 7 edges
7. `setText()` - 7 edges
8. `setText()` - 7 edges
9. `getReact()` - 7 edges
10. `boot()` - 7 edges

## Surprising Connections (you probably didn't know these)
- `boot()` --calls--> `getReact()`  [EXTRACTED]
  support.js → support.js  _Bridges community 3 → community 5_
- `walkFor()` --calls--> `getReact()`  [EXTRACTED]
  support.js → support.js  _Bridges community 3 → community 1_
- `compileAttr()` --calls--> `resolve()`  [EXTRACTED]
  support.js → support.js  _Bridges community 0 → community 1_
- `walkText()` --calls--> `resolve()`  [EXTRACTED]
  support.js → support.js  _Bridges community 0 → community 3_
- `compileTemplate()` --calls--> `walkChildren()`  [EXTRACTED]
  support.js → support.js  _Bridges community 7 → community 1_

## Import Cycles
- None detected.

## Communities (10 total, 0 thin omitted)

### Community 0 - "support.js"
Cohesion: 0.12
Nodes (11): createExternalModules(), createHelmetManager(), createRegistry(), createRuntime(), findTopLevelEquality(), isElementClass(), isRenderableType(), parensWrapWhole() (+3 more)

### Community 1 - "walk"
Cohesion: 0.33
Nodes (13): collectProps(), compileAttr(), contentKey(), cssToObj(), hostPositionStyle(), kebabToCamel(), walk(), walkChildren() (+5 more)

### Community 2 - "site.js"
Cohesion: 0.19
Nodes (18): applyRoomContent(), clearTilt(), createMessage(), motionIsOff(), renderAxisList(), renderHubLineage(), renderHubMessages(), renderHubRoom() (+10 more)

### Community 3 - "getReact"
Cohesion: 0.40
Nodes (5): createComponentFactory(), evalDcLogic(), getReact(), walkText(), warnUnresolved()

### Community 4 - "createRuntime"
Cohesion: 0.20
Nodes (22): activateRoom(), announce(), createAttachment(), createChatMessage(), createCrosscutStrip(), createDivider(), createEvent(), createMessageActions() (+14 more)

### Community 5 - "boot"
Cohesion: 0.22
Nodes (10): boot(), createStreamTracker(), dcNameFromPath(), getReactDOM(), init(), parseDataProps(), parseDcDocument(), parseDcText() (+2 more)

### Community 6 - "loadReactUmd"
Cohesion: 0.67
Nodes (3): cdnScriptFor(), loadReactUmd(), loadScript()

### Community 7 - "compileTemplate"
Cohesion: 0.67
Nodes (3): compileTemplate(), encodeCamelAttrs(), encodeCase()

### Community 8 - "rootNameForDocument"
Cohesion: 0.67
Nodes (4): createPseudoSheet(), importantify(), scanUnquotedUrl(), stripComments()

### Community 9 - "site.js"
Cohesion: 0.19
Nodes (18): applyRoomContent(), clearTilt(), createMessage(), motionIsOff(), renderAxisList(), renderHubLineage(), renderHubMessages(), renderHubRoom() (+10 more)

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `createRuntime()` connect `support.js` to `rootNameForDocument`, `getReact`, `boot`?**
  _High betweenness centrality (0.002) - this node is a cross-community bridge._
- **Should `support.js` be split into smaller, more focused modules?**
  _Cohesion score 0.11594202898550725 - nodes in this community are weakly interconnected._