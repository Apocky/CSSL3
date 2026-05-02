# 18 · GEAR-MODULARITY-CANONICAL · Coherence-Engine 13-Axis · Component-Rarity-Affix
# § meta
  spec : ⌈18_GEAR_MODULARITY_CANONICAL⌉
  status : ✓ canonical · supersedes ⌈CSSLv3/GDDs/GEAR_RARITY_SYSTEM.csl⌉
  authority : DEPRECATED-The-Infinite-Labyrinth/GDDs/ARTIFACT_ASCENSION_SPEC.md
            + DEPRECATED-The-Infinite-Labyrinth/GDDs/ENTITY_BODY_SYSTEM_SPEC.md
            + DEPRECATED-The-Infinite-Labyrinth/GDDs/CHEMISTRY_MAGIC_CRAFTING_BIBLE.md
            + DEPRECATED-The-Infinite-Labyrinth/GDDs/Master-Plan.md
            + DEPRECATED-The-Infinite-Labyrinth/GDDs/THE_MANTLE.md
  notation : CSLv3-glyph-dense
  W! cross-ref-by-LOC into-canonical-source ¬ paraphrase-loose
  ∀ formulas + thresholds + magic-numbers ⊂ canonical-source ← preserve-exact

---

# § 1 · HISTORICAL-CONTEXT

## I> canonical-authority-claim
  DEPRECATED-Infinite-Labyrinth/GDDs/ARTIFACT_ASCENSION_SPEC.md
    ≡ canonical-gear-modularity-spec ∀ Apocky-LoA
    @ V1.0 · "Design-Complete · Implementation-Ready" «ARTIFACT_ASCENSION_SPEC:5-6»
    ⊑ 1731-LOC · 12-section · §1-§12 + Conclusion

  DEPRECATED-Infinite-Labyrinth/GDDs/ENTITY_BODY_SYSTEM_SPEC.md
    ≡ canonical-component-anatomy-spec ∀ Apocky-LoA
    ⊑ "Bodies are NOT stat blocks with visual skins" «ENTITY_BODY_SYSTEM_SPEC:8»
    ⊑ "directed tree of BodyPart components" «ENTITY_BODY_SYSTEM_SPEC:22»

  DEPRECATED-Infinite-Labyrinth/GDDs/CHEMISTRY_MAGIC_CRAFTING_BIBLE.md
    ⊑ §5.3 Component-Rarity-Affix-System «CHEMISTRY:493-527»
    ≡ canonical-modularity-system

## I> CSSLv3-stub-status
  ⌈CSSLv3/GDDs/GEAR_RARITY_SYSTEM.csl⌉
    ≡ V13-bootstrap-only ¬ canonical
    @ origin : pre-DEPRECATED-IL-rediscovery
    role : cosmetic-overlay-layer ¬ truth-source
    'p deprecated → composes-onto-canonical-13-axis ¬ replaces

## I> apocky-correction-2026-05-01 (T11-W14-I)
  Apocky « ¬ canonical · DEPRECATED-IL-is »
    'p applies-to ⊑ DEPRECATED-Infinite-Labyrinth/GDDs/* ∀ gear+ascension+body+chemistry
  ✗ prior-cssl-stub-treatment-as-canonical = mistake
  ✓ this-doc = correct-canonical-re-author

---

# § 2 · 13-AXIS COHERENCE-ENGINE

## I> source : ARTIFACT_ASCENSION_SPEC «:27-46»
  ⊑ 13 axes ∀ item-composition
  Axis-13 = Coherence-Score = capstone reading-axes-1-12

## I> axis-table (canonical)
  | # | axis              | source-system           | reference-LOC                                        |
  |---|-------------------|-------------------------|------------------------------------------------------|
  | 1 | Form              | ItemDefinition-table    | «ARTIFACT_ASCENSION_SPEC:33»                         |
  | 2 | Components[]      | ItemAssembly hot-swap   | «ARTIFACT_ASCENSION_SPEC:34»                         |
  | 3 | Material          | MaterialDefinition/comp | «ARTIFACT_ASCENSION_SPEC:35»                         |
  | 4 | ComponentRarity   | Common→Legendary/comp   | «ARTIFACT_ASCENSION_SPEC:36» «CHEMISTRY:514-522»     |
  | 5 | Affixes           | Rolled-per-component    | «ARTIFACT_ASCENSION_SPEC:37»                         |
  | 6 | CulturalDoctrine  | CombatDoctrine/faction  | «ARTIFACT_ASCENSION_SPEC:38»                         |
  | 7 | Quality           | CraftingSystem-formula  | «ARTIFACT_ASCENSION_SPEC:39» «CHEMISTRY:614-616»     |
  | 8 | LatticeSockets[]  | Modular-Spell-Cartridges| «ARTIFACT_ASCENSION_SPEC:40»                         |
  | 9 | Resonance         | Per-element-attunement  | «ARTIFACT_ASCENSION_SPEC:41» «CHEMISTRY:368-385»     |
  | 10| Patina            | Biography-driven-passive| «ARTIFACT_ASCENSION_SPEC:42»                         |
  | 11| Corruption-Blessing| spectrum -1.0 → +1.0   | «ARTIFACT_ASCENSION_SPEC:43»                         |
  | 12| Cross-Genre       | Genre-bridging-penalty  | «ARTIFACT_ASCENSION_SPEC:44»                         |
  | 13| CoherenceScore    | THIS-DOC §3-§5          | «ARTIFACT_ASCENSION_SPEC:45-47»                      |

## I> per-axis canonical-formula

### § 2.1 axis-1 Form
  ↦ ItemDefinition.Id : invariant-template
  ↦ defines : equip-slot + base-volume + attack-animation «Master-Plan:79»
  axiom : "form + substance" ← IVAN «Master-Plan:9»
  composition : Form × Components × Material × Rarity × Affix
    = combinatorial-explosion ¬ per-item-defined «CHEMISTRY:524»

### § 2.2 axis-2 Components[]
  ↦ ItemComponent[] hot-swappable «ARTIFACT_ASCENSION_SPEC:501-512»
  fields : Type · MaterialId · Volume · DamageContribution · DefenseContribution
         · ComponentRarity · Affixes[] · Durability
  durability-formula : Material.Hardness × 25.0 «CHEMISTRY:512»
  quality-gated-assembly : Guard@q2 · Pommel@q3 · Edge@q5 · Fuller@q7 «CHEMISTRY:526»
  ⊑ "Higher quality items have MORE component slots, not just better base stats" «CHEMISTRY:526»

### § 2.3 axis-3 Material
  ↦ continuous-float-properties «CHEMISTRY:55-71»
  W! ¬ stat-block ◐ material-instance
  16-properties-canonical :
    Density · MeltingPoint · BoilingPoint · Hardness · Conductivity
    Flammability · AcidResistance · FatigueRate · Reactivity · Toxicity
    MagicAffinity · Opacity · Elasticity · Solubility · Volatility
  target : 60+ base-materials · 20+ alloys «CHEMISTRY:81»
  axiom : "Everything is Material" «THE_MANTLE:143» «Master-Plan:9»

### § 2.4 axis-4 ComponentRarity (Q-06 APPLIED · 8-tier)
  ↦ enum 8-tier (Apocky-canonical 2026-05-01 · supersedes-CHEMISTRY-5-tier)
  verbatim-Apocky : "Add mythic, prismatic, chaotic, in that order of ascending rarity."
  | tier       | affix-slots | drop-rate-bps | drop-rate-%   | LOC-ref · canonical-source       |
  |-----------|-------------|---------------|----------------|----------------------------------|
  | Common     | 0           |  60_000       | 60.000%       | Q-06 · «:524-525»                 |
  | Uncommon   | 1           |  25_000       | 25.000%       | Q-06 · «:524-525»                 |
  | Rare       | 2           |  10_000       | 10.000%       | Q-06 · «:524-525»                 |
  | Epic       | 3           |   4_000       |  4.000%       | Q-06 · «:524-525»                 |
  | Legendary  | 4           |     900       |  0.900%       | Q-06 · DEPRECATED-IL canonical    |
  | Mythic     | 5           |      90       |  0.090%       | Q-06 · charter-§5.8-extension     |
  | Prismatic  | 6           |       9       |  0.009%       | Q-06 · NEW · Apocky-2026-05-01    |
  | Chaotic    | 7           |       1       |  0.001%       | Q-06 · NEW · Apocky-2026-05-01    |
  | TOTAL      |             | 100_000       |100.000%       |                                   |
  ⊑ sum-check : 60_000 + 25_000 + 10_000 + 4_000 + 900 + 90 + 9 + 1 = 100_000 bps ✓
  ⊑ "Why affixes on components, not items" «CHEMISTRY:524»
  axiom : Common-Gold-Crossguard = pure-gold-stats
        ; Rare-Gold-Crossguard = same-material + 2-rolled-magical-properties
        ; Chaotic-Gold-Crossguard = 7-affix-slot + Σ-mask-randomized-affix-pool
        ; → completely-different-gameplay-value «CHEMISTRY:524»
  ⊑ NEW-tier-semantics :
    Mythic    = drop-only OR Legendary-bond + 5×Epic-chain (◐ Apocky-confirm)
    Prismatic = drop-only · ¬ transmute · 6 affix-slots · multi-element-resonance
    Chaotic   = drop-only · ¬ transmute · 7 affix-slots · Σ-mask-randomized-pool
                  ⊕ "wildcard" semantics : affix-pool drawn-from-ALL-tiers
                  ⊕ unique-Epithet-trigger @ Coherence-Score ≥ 95
  ⊑ glyph-slot-table-canonical (Q-06 + GDD § GLYPH-SLOTS extension) :
    | rarity     | lower | upper | source                                    |
    |-----------|-------|-------|-------------------------------------------|
    | Common     |   0   |   0   | GDD § GLYPH-SLOTS                         |
    | Uncommon   |   0   |   1   | GDD § GLYPH-SLOTS                         |
    | Rare       |   1   |   1   | GDD § GLYPH-SLOTS                         |
    | Epic       |   1   |   2   | GDD § GLYPH-SLOTS                         |
    | Legendary  |   2   |   3   | GDD § GLYPH-SLOTS                         |
    | Mythic     |   3   |   4   | extended-from-3 ← Apocky-2026-05-01       |
    | Prismatic  |   4   |   5   | NEW · Apocky-2026-05-01                   |
    | Chaotic    |   5   |   6   | NEW · Apocky-2026-05-01                   |

### § 2.5 axis-5 Affixes
  ↦ live-on-components ¬ items «CHEMISTRY:827»
  rejected : item-level-affixes ✗ «CHEMISTRY:827»
  rationale : "component-level affixes create exponentially more variety" «CHEMISTRY:827»
  interactions : ElementalChain · ConflictPair · StatStack
                · ConditionalTrigger · ThresholdContribution «ARTIFACT_ASCENSION_SPEC:660-697»

### § 2.6 axis-6 Cultural-Doctrine
  ↦ FactionDoctrine + CombatDoctrine
  doctrines : Militaristic · Theocratic · Technocratic «ENTITY_BODY_SYSTEM_SPEC:380-393»
  doctrine-fields : PreferredWeightClass · FavoredElement · AggressionBias «ARTIFACT_ASCENSION_SPEC:603-630»
  faction-relations : Alliance · SuccessorState · TradePartners · War · NoContact «ARTIFACT_ASCENSION_SPEC:553-573»

### § 2.7 axis-7 Quality
  ↦ CraftingSystem-formula «CHEMISTRY:614-616»
  formula : 0.30 × material-hardness + 0.30 × forge-level
          + 0.20 × player-affinity + 0.20 × tool-quality
  tiers : Crude(0.7×) · Common(1.0×) · Fine(1.15×) · Superior(1.3×) · Legendary(1.5×) «CHEMISTRY:616»

### § 2.8 axis-8 Lattice-Sockets
  ↦ modular-Spell-Cartridges ↦ inscription-system «CHEMISTRY:618-635»
  inscription-complexity-by-surface :
    small-(rings·arrowheads) : 1-2-node-lattices «CHEMISTRY:631»
    medium-(swords·shields)  : 3-5-node-lattices «CHEMISTRY:632»
    large-(staves·breastplates) : 6-10-node-lattices «CHEMISTRY:633»
  distinction : component-affixes = passive-stats
              ; inscription = active-spell-effects «CHEMISTRY:635»

### § 2.9 axis-9 Resonance
  ↦ ResonanceState : Dictionary<ElementType, float> Attunements «CHEMISTRY:368-385»
  buildup : slow + persistent ; entire-run might-reach 0.2 «CHEMISTRY:382»
  conflict-rules : Fire suppresses Water + vice-versa
                 ; Void suppresses all-others «CHEMISTRY:384»
  axiom : "creates legendary items through use, not just through crafting stats" «CHEMISTRY:382»

### § 2.10 axis-10 Patina
  ↦ biography-derived passive-effects
  source : ItemBiography ring-buffer events «ARTIFACT_ASCENSION_SPEC:151-211»
  W! biography-driven ¬ random-roll
  see : § 5 ITEM-BIOGRAPHY

### § 2.11 axis-11 Corruption-Blessing
  ↦ spectrum [-1.0 ← Void · +1.0 ← Light]
  thresholds :
    |C/B| > 0.3 → counts-as Void/Light source «ARTIFACT_ASCENSION_SPEC:363-367»
    |C/B| > 0.5 → triggers Paradox-Tension if-opposing-affix «ARTIFACT_ASCENSION_SPEC:792-805»

### § 2.12 axis-12 Cross-Genre
  ↦ genre-bridging with-compatibility-penalty
  genres : Medieval · SciFi · Cyberpunk · Eldritch · Flora · Construct «ENTITY_BODY_SYSTEM_SPEC:336-340»
  N-genres-on-item : base-2pts + (N-2)×1pt → Paradox-Tension «ARTIFACT_ASCENSION_SPEC:773-777»

### § 2.13 axis-13 Coherence-Score (CAPSTONE)
  ↦ reads-axes-1-12 ; if-crosses-AscensionThreshold → generates-Epithet «ARTIFACT_ASCENSION_SPEC:46-47»
  threshold : 70.0 (of-max-100) «ARTIFACT_ASCENSION_SPEC:830»
  ¬ randomness ; identical-state → identical-score-and-Epithet «ARTIFACT_ASCENSION_SPEC:8»
  see : § 3 6-DIM-HARMONY · § 4 ASCENSION

---

# § 3 · 6-DIM HARMONY-DIMENSIONS

## I> source : ARTIFACT_ASCENSION_SPEC «:60-108»
  ⊑ CoherenceEvaluation struct
  ∀ scoring-functions = pure ; deterministic ; testable «ARTIFACT_ASCENSION_SPEC:319»

## I> dim-table (canonical-max + tie-priority)
  | # | dimension          | range  | tie-priority | source-axes-read              |
  |---|--------------------|--------|--------------|-------------------------------|
  | 1 | ElementalPurity    | 0-30   | 4            | Affixes + Resonance + Cartridge + Corruption + Patina |
  | 2 | MaterialHarmony    | 0-15   | 1            | Components.Materials          |
  | 3 | CulturalCoherence  | 0-15   | 3            | Components.CulturalOrigin + Substrate |
  | 4 | AffixSynergyDepth  | 0-20   | 5            | Affix-Interaction-rules       |
  | 5 | BiographyWeight    | 0-10   | 2            | ItemBiography ring-buffer     |
  | 6 | ParadoxTension     | 0-10   | 6 (highest)  | Conflicts + cross-genre + corruption-paradox |

  total-max : 30 + 15 + 15 + 20 + 10 + 10 = 100 «ARTIFACT_ASCENSION_SPEC:69-71»
  threshold : 70 ⇒ Ascension «ARTIFACT_ASCENSION_SPEC:830»

## § 3.1 ElementalPurity (0-30)

  fn : ElementalPurityScorer.Score(item) «ARTIFACT_ASCENSION_SPEC:325-398»
  algorithm-canonical :
    1. count-elemental-sources/element ∀ axes :
       affix → 1.0/source
       resonance > 0.05 → attunement × 2.0 (max-2.0)
       active-cartridge → 1.5
       corruption < -0.3 → |C| → Void-source
       blessing > 0.3   → +B → Light-source
       patina-element   → 0.5
    2. dominant-element + concentration-ratio :
       purityRatio = dominant/total
       sourceCount = dominant.value
    3. depthScore = sqrt(min(sourceCount/8.0, 1.0))
    4. score = clamp(purityRatio × 20.0 + depthScore × 10.0, 0, 30)

  scoring-table-canonical «ARTIFACT_ASCENSION_SPEC:404-409» :
    1-fire-affix-only          → ~23.5
    4-fire+0.4-Resonance+Cartridge → ~28.9
    3-fire+1-water             → ~22.1
    2-fire+2-water+1-earth     → ~15.9
    no-elemental-affixes       → 0.0

## § 3.2 MaterialHarmony (0-15)

  fn : MaterialHarmonyScorer.Score(item) «ARTIFACT_ASCENSION_SPEC:418-503»
  3-factor-decomposition :
    factor-1 : FamilyCohesion (0-6)
      all-same-material → 6.0
      else → largestFamilyRatio × 5.0
    factor-2 : PhysicalComplementarity (0-5)
      density-ratio strike↔balance ∈ [1.5, 5.0] → +1.0/pair
      structural-hardness-match → +ratio×0.5/pair
    factor-3 : ChemicalStability (0-4 · penalty-based · start-at-4)
      Corrosive   → -2.0 (Iron + Acid-producing)
      Unstable    → -1.5 (thermally-incompatible)
      Degrading   → -0.5 (slow-degradation)
      Synergistic → +0.5
      Inert       → no-penalty

  material-families-canonical «ARTIFACT_ASCENSION_SPEC:419-429» :
    FerricMetal   : Iron · Steel · Orichalcum
    PreciousMetal : Gold · Silver · Platinum
    ExoticMetal   : Mithril · Adamantine · Arcanite
    Organic       : Wood · Bone · Leather · Chitin
    Mineral       : Stone · Obsidian · Crystal · Diamond
    Eldritch      : Voidite · Aetherstone · FleshSteel
    Synthetic     : Plasteel · NanoCarbon · BioGel

## § 3.3 CulturalCoherence (0-15)

  fn : CulturalCoherenceScorer.Score(item, substrate) «ARTIFACT_ASCENSION_SPEC:511-635»
  3-factor-decomposition :
    factor-1 : FactionUnity (0-8)
      score = dominantFactionRatio × 8.0
    factor-2 : HistoricalRelationship (0-5 · per-faction-pair)
      Alliance/SuccessorState → +2.5
      TradePartners           → +1.5
      War                     → -1.0 (boosts ParadoxTension!)
      NoContact               → -0.5
      single-faction          → +3.0 default
    factor-3 : DoctrineAlignment (0-2)
      align-by-WeightClass + FavoredElement + AggressionBias

  axiom : substrate-regenerates-per-run
        ⇒ cultural-coherence ¬ checklist-able «ARTIFACT_ASCENSION_SPEC:1364»

## § 3.4 AffixSynergyDepth (0-20)

  fn : AffixSynergyScorer.Score(item) «ARTIFACT_ASCENSION_SPEC:644-706»
  interaction-types + scores :
    ElementalChain :
      2-link → 2.0 ; 3-link → 4.0 ; 4-link → 6.0 ; 5+-link → 8.0
    ConflictPair          : 5.0/instance (most-valuable)
    StatStack             : 0.5/source
    ConditionalTrigger    : 4.0/instance
    ThresholdContribution : 1.0/source-on-this-item
  diversity-bonus : +1.0/distinct-interaction-type (max-5)

## § 3.5 BiographyWeight (0-10)

  fn : BiographyWeightScorer.Score(bio) «ARTIFACT_ASCENSION_SPEC:714-746»
  scoring-canonical :
    NemesisKills        → +2.0/each
    NemesisDeaths       → +0.5/each
    NemesisRecovered    → +1.5/each (recovery-arc-strong)
    NearDeathSurvivals  → +0.5/each (cap-2.0)
    distinctBiomes      → +0.3/each (cap-1.5)
    RunsSurvived        → +0.5/each (cap-1.5)
    totalKills-log2     → ×0.3 (cap-1.5)

## § 3.6 ParadoxTension (0-10)

  fn : ParadoxTensionScorer.Score(item, substrate) «ARTIFACT_ASCENSION_SPEC:754-822»
  scoring-canonical :
    ConflictPairs           → +2.0/each
    cross-genre (≥2)        → +2.0 base + 1.0/extra-genre
    enemy-faction-merge     → +1.5/pair
    corruption-with-life-affix    → +2.0
    blessing-with-void-affix      → +2.0
    material-reactivity-survived  → +1.0/pair
    (cost MaterialHarmony · gain ParadoxTension ← productive-tension)

## § 3.7 CompositeEvaluator (canonical)

```
fn Evaluate(item, substrate) ↦ CoherenceEvaluation :
  ElementalPurity   ← ElementalPurityScorer.Score(item)
  MaterialHarmony   ← MaterialHarmonyScorer.Score(item)
  CulturalCoherence ← CulturalCoherenceScorer.Score(item, substrate)
  AffixSynergyDepth ← AffixSynergyScorer.Score(item)
  BiographyWeight   ← BiographyWeightScorer.Score(item.Biography)
  ParadoxTension    ← ParadoxTensionScorer.Score(item, substrate)

const AscensionThreshold = 70.0 «ARTIFACT_ASCENSION_SPEC:830»
```

---

# § 4 · ASCENSION (canonical)

## I> trigger-types «ARTIFACT_ASCENSION_SPEC:129-141»
  Crafting           — assembly-completes
  ComponentSwap      — hot-swap-mutation
  AuraApplication    — Aura-crystal-fills-slot
  CartridgeSocket    — Spell-Cartridge-installed
  ResonanceGrowth    — Resonance crosses 0.1-increment
  PatinaAccrual      — Biography-event-logged
  CorruptionShift    — C/B crosses 0.1-increment
  TransmuterModified — material-transmuted
  PreAscended        — Combinatorial-Factory-historical

## I> deterministic-Epithet-pipeline «ARTIFACT_ASCENSION_SPEC:881-1107»
  step-1 : DominantHarmony → EpithetCategory (1:1) :
    ElementalPurity   → Incarnation
    MaterialHarmony   → Perfection
    CulturalCoherence → Legacy
    AffixSynergy      → ResonanceCascade
    BiographyWeight   → Legendary
    ParadoxTension    → Anomaly
  step-2 : effect-selection per-category × item.PrimaryRole
  step-3 : magnitude scaling ← Coherence-Score
  step-4 : name generation (template-tier always · LLM-tier optional)
  step-5 : visuals ← EpithetVisuals struct (glow + particle + iridescence)

## I> magnitude-scaling-canonical «ARTIFACT_ASCENSION_SPEC:1115-1119»
```
BaseMagnitude       = (DimensionScore / DimensionMax) × 0.25
CoherenceMultiplier = 1.0 + ((TotalScore - 70) / 30) × 0.5
EffectiveMagnitude  = BaseMagnitude × CoherenceMultiplier
```
  @ threshold-70  : multiplier = 1.0 (~15%-effect)
  @ score-85      : multiplier = 1.25 (~19%)
  @ perfect-100   : multiplier = 1.5  (~25%)
  secondary-effect : halved ; only-if score ≥ 50%-of-max «ARTIFACT_ASCENSION_SPEC:914-924»

## I> EpithetEffectType-canonical-30+ «ARTIFACT_ASCENSION_SPEC:259-296»
  Incarnation     : ResistancePiercing · ElementalAura · ElementalAbsorption · EnvironmentalResonance
  Perfection      : DurabilityMultiplier · WeightOptimization · SeamlessForm · ImpactTransfer
  Legacy          : HistoricalEcho · CulturalFavor · AncestralMemory · TacticalDoctrine
  ResonanceCascade: ChainAmplification · ConflictCrystallization · ThresholdReduction · CascadeProc
  Legendary       : NemesisBane · SurvivorInstinct · PilgrimWisdom · VeteranPresence
  Anomaly         : ElementalOscillation · GenreBleed · CorruptionPulse · RealityWarp · StabilizedParadox

## I> never-de-Ascend «ARTIFACT_ASCENSION_SPEC:1374-1393»
  axiom : "items never de-Ascend" «:1374»
  if post-swap-Coherence < threshold :
    Epithet persists ; magnitude scales-down (60%-floor)
  ⊑ "creates a genuine cost to post-Ascension modifications without the feel-bad of losing Ascension entirely" «:1376»

## I> ascension-rate-targets «ARTIFACT_ASCENSION_SPEC:1398-1404»
  random-loot-no-crafting   : <1% items approach-threshold
  casual-crafter            : ~3-5% endgame-items Ascend
  dedicated-buildcrafter    : ~15-25% serious-attempts Ascend
  min-maxer                 : ~40-50% deliberate-attempts Ascend

---

# § 5 · ITEM-BIOGRAPHY (canonical)

## I> source : ARTIFACT_ASCENSION_SPEC «:151-211»

## I> ring-buffer-canonical
  capacity : MaxEntries = 64 «:153»
  oldest-evicted-when-full
  TotalEventsWitnessed : lifetime-counter (¬ resets-on-eviction)

## I> aggregated-counters (efficient-Patina+Coherence-queries) «:159-168»
  ElementalKills      : Dictionary<ElementType, int>     ← kills-by-element-dealt
  ElementalBlocks     : Dictionary<ElementType, int>     ← blocks-by-element-received
  CreatureTypeKills   : Dictionary<CreatureType, int>    ← kills-by-taxonomy
  FactionKills        : Dictionary<FactionId, int>       ← kills-by-victim-faction
  BiomesTraversed     : Dictionary<BiomeType, int>       ← rooms-visited
  NearDeathSurvivals  : int                              ← times-equipped-when-HP<5%
  NemesisKills        : int                              ← named-Nemesis-slain
  NemesisDeaths       : int                              ← times-equipped-when-Nemesis-killed-player
  RunsSurvived        : int                              ← complete-runs-extracted
  OwnersCount         : int                              ← distinct-players (multiplayer)

## I> BiographyEventType-canonical-categories «:178-211»
  combat-events :
    Kill · NemesisKill · NemesisDeath · NemesisRecovered
    NearDeathSurvival · MassKill · BossKill
  environmental-events :
    BiomeFirstEntry · HazardSurvival · ExtremeTempExposure · GenreCrossing
  social-events :
    FactionMemberKill · DeityOfferingPresent · TradeHandoff
  crafting-events :
    ComponentReplaced · AuraApplied · CartridgeChanged · Transmuted
  meta-events :
    RunExtracted · RunDeath · AscensionAchieved

## I> BiographyEntry-struct «:172-177»
  Tick         : ulong       ← when
  EventType    : enum        ← what
  EventData    : uint        ← context-dependent-payload
  Significance : float [0,1] ← how-dramatic
  ⊑ Significance ≥ 0.5 → triggers-Ascension-check «ARTIFACT_ASCENSION_SPEC:1492»

---

# § 6 · COMPONENT-RARITY-AFFIX-SYSTEM (canonical-modularity)

## I> source : CHEMISTRY_MAGIC_CRAFTING_BIBLE.md §5.3 «:493-527»
  W! THIS-IS-the-canonical-modularity ¬ CSSL3-stub

## I> ItemComponent-canonical-struct «CHEMISTRY:501-512»
```
class ItemComponent :
  Type                : ComponentType (Blade · Guard · Grip · Pommel · Edge · Fuller · ...)
  Material            : MaterialId
  Volume              : float
  DamageContribution  : float
  DefenseContribution : float
  Rarity              : ComponentRarity
  Affixes             : List<Affix>     ← 0-4 by-rarity
  Durability          : Material.Hardness × 25.0
```

## I> ComponentRarity-enum (Q-06 APPLIED · 8-tier · supersedes 5-tier)
verbatim-Apocky 2026-05-01 : "Add mythic, prismatic, chaotic, in that order of ascending rarity."
```
enum ComponentRarity :
  Common    ← 0 affix-slots ← pure-material-stats        ← 60.000% drop
  Uncommon  ← 1 affix-slot                               ← 25.000% drop
  Rare      ← 2 affix-slots                              ← 10.000% drop
  Epic      ← 3 affix-slots                              ←  4.000% drop
  Legendary ← 4 affix-slots                              ←  0.900% drop
  Mythic    ← 5 affix-slots ← drop-or-bond-only          ←  0.090% drop
  Prismatic ← 6 affix-slots ← drop-only · multi-element  ←  0.009% drop
  Chaotic   ← 7 affix-slots ← drop-only · wildcard-pool  ←  0.001% drop
```
sum : 60.000 + 25.000 + 10.000 + 4.000 + 0.900 + 0.090 + 0.009 + 0.001 = 100.000% ✓
prior-CHEMISTRY-5-tier «:514-522» = subset of-canonical-8-tier (compatibility-preserved)

## I> the-canonical-axiom «CHEMISTRY:524»
  ⊑ "Why affixes on components, not items: A Common Gold Crossguard is just gold stats.
     A Rare Gold Crossguard has 2 rolled magical properties — same material, different rarity,
     completely different gameplay value. This means Form × Component × Material × Rarity × Affix
     creates a combinatorial explosion without needing to define each item individually.
     Cross-component affix interactions (stacking, synergy, conflict, threshold bonuses)
     create emergent item identities."

## I> quality-gated-assembly «CHEMISTRY:526»
  ⊑ "Components unlock at quality thresholds — Guard@q2, Pommel@q3, Edge@q5, Fuller@q7.
     Higher quality items have MORE component slots, not just better base stats."

## I> deconstruct-to-learn (Formbook) «CHEMISTRY:530-549»
  ⊑ "Blueprints were explicitly rejected" «:529»
  loop : find-unknown-form → deconstruct@TheBreaker → learn-Form-permanently
  ⊑ "every trash drop valuable" «:548»
  ⊑ "drives exploration : new biomes and genres contain Forms you've never seen" «:549»

## I> salvage-asymmetry «CHEMISTRY:556-558»
  ⊑ "Found/looted items → pick ONE component to salvage, rest destroyed."
  ⊑ "Player-crafted items → full deconstruction, ALL components preserved."

## I> Aura-Essence-affix-economy «CHEMISTRY:564-577»
  obtain : 20-30%-chance-per-destroyed-component yields-Aura-crystal
  apply  : @TheResonanceCrucible fills-empty-affix-slot on-existing-component
  endgame-loop : find-good-affixes → deconstruct → apply-Aura → chase-better-combinations
  ⊑ "Nothing is truly worthless." «:577»

## I> 7-Hub-Workbenches «CHEMISTRY:583-591»
  | workbench           | domain          | operations                                    |
  |---------------------|-----------------|-----------------------------------------------|
  | Worldforge          | metals          | smelting · forging · tempering · alloys       |
  | LivingBench         | organics        | bone/wood/leather/chitin/fungal               |
  | LoomOfFates         | cloth/fiber     | weaving · stitching · dyeing · enchant-thread |
  | Fabricator          | tech/sci-fi     | mechanical/electrical assembly                |
  | Breaker             | deconstruction  | learn-form + salvage + Aura-crystal-chance    |
  | ResonanceCrucible   | Aura-imbuing    | apply-Aura-crystal-to-empty-affix-slot        |
  | Transmuter          | material-conv   | alloy + purify + transmute (Int-gated)        |

## I> ENTITY-BODY-SYSTEM extends-ItemComposition «ENTITY_BODY_SYSTEM_SPEC:23-27»
  ⊑ "Bodies are compositional ... Capabilities emerge from which parts exist and what they're made of."
  ⊑ "Bodies are material ... Material properties (hardness, density, conductivity, flammability,
     acid resistance) directly determine that part's durability, weight contribution, and special
     interactions with the Chemistry Engine."
  ⊑ "Prosthetics are items ... A prosthetic arm can Ascend." «:27»

  body-tree-canonical : Torso = root ; directed-tree of-BodyPart-nodes
  prosthetic-tiers «ENTITY_BODY_SYSTEM_SPEC:430-446» :
    Crude     : 50-70% efficiency · -20% stats · early-game
    Standard  : 80-100% · -5% to +5%
    Advanced  : 100-130% · +10% to +30%
    Divine    : 110-150% · +15% to +50% in-sacred-domain
    Anomalous : 80-160% (variable) · cross-genre

---

# § 7 · INTEGRATION-PROPOSAL → ω-FIELD-CELLS

## I> mapping-canonical-13-axes ↦ Infinity-Engine-substrate

The 13-axis Coherence-Engine maps cleanly onto the cssl-substrate-omega-field architecture
(specs/30_SUBSTRATE_v2.csl) by treating each item as a multi-cell ω-field-region :

  axis-1 Form              ↦ omega-cell.shape-prior (KAN-substrate-runtime)
  axis-2 Components[]      ↦ omega-cell-children (parent-child-Σ-mask-edges)
  axis-3 Material          ↦ omega-cell.material-tag (ContinuousMaterialField)
  axis-4 ComponentRarity   ↦ omega-cell.rarity-tier-attribute
  axis-5 Affixes           ↦ Σ-mask per-component-cell (sparse-affix-overlay)
  axis-6 CulturalDoctrine  ↦ omega-cell.faction-provenance-edge
  axis-7 Quality           ↦ omega-cell.quality-scalar (0.7 ← 1.5)
  axis-8 LatticeSockets    ↦ omega-cell.spell-cartridge-region (sub-region-Σ-mask)
  axis-9 Resonance         ↦ omega-cell.attunement-vector (per-element-fixed-point)
  axis-10 Patina           ↦ omega-cell.biography-overlay (ring-buffer-as-bit-pack)
  axis-11 Corruption-Bless ↦ omega-cell.corruption-scalar [-1, +1]
  axis-12 Cross-Genre      ↦ omega-cell.genre-bitset (multi-genre-allowed)
  axis-13 CoherenceScore   ↦ DERIVED-cell.computed-from-1-12 (KAN-evaluator)

  cell-region-equivalence :
    one-item ≡ 1-N omega-cells (1 root + N components)
    Σ-mask edges encode-component-attachment
    KAN-substrate-runtime evaluates Coherence as-pure-fn over-cell-region
    deterministic ∵ ω-field-as-truth ; ¬ runtime-randomness

  6-novelty-path-multiplicative-composition :
    ↦ each-Harmony-Dimension = 1-novelty-path
    ↦ Coherence-evaluator = multiplicative-compose 6-paths
    ↦ ¬ requires new-substrate-primitives ; uses-existing-multiplicative-fanout

  bit-pack-record-discipline (Sawyer/Pokémon-OG) :
    ItemBiography ring-buffer = 64 entries × {tick:u32, type:u8, data:u32, sig:u8} = ~10-bytes/entry
    aggregated-counters = bit-packed Dictionary<small-enum, u16-counter>
    AscensionState = single-row : {bool + u32-score + u8-dim + u8-dim + EpithetId-u32 + tick + trigger-u8}

## I> stage-0 fallback-discipline
  W! ∀ Coherence-evaluators MUST have stage-0-Rust-fallback
  ∵ stage-0-Rust ≡ valid-substrate-host ; CSSL-source-compiles-to-Rust-AOT
  ¬ block ω-field-cell-arch on-future-substrate-primitives ;
  W! land Rust-stage-0 first ↦ migrate-to-CSSL-source-iteratively

---

# § 8 · DEPRECATION-OF-CSSL3-STUB

## I> existing-stub : ⌈CSSLv3/GDDs/GEAR_RARITY_SYSTEM.csl⌉

  status : V13-bootstrap-only
  authored : pre-DEPRECATED-IL-rediscovery
  apocky-correction : ¬ canonical · DEPRECATED-IL-is

## I> retirement-strategy

  ✗ ¬ delete-stub ← preserves-history + W13-8-cosmetic-overlay-may-compose
  ✓ mark-as : "cosmetic-overlay-layer · composes-onto-canonical-13-axis"
  ✓ canonical-pointer : ALL gear-modularity-questions → THIS-doc + DEPRECATED-IL-source
  ✓ if-stub-conflicts-with-canonical : canonical-WINS

## I> compose-not-replace
  CSSL3-stub-rarity-cosmetics may-compose :
    canonical : ComponentRarity = {Common, Uncommon, Rare, Epic, Legendary, Mythic, Prismatic, Chaotic}
                                  = AFFIX-SLOT-COUNT (Q-06 · 8-tier)
    stub-cosmetic : { glow-color, particle-density, name-prefix-pool } per-rarity = visual-overlay
    rule : stub-cosmetic ⊑ canonical-rarity ; ¬ stub-cosmetic-defines-mechanics

## I> migration-todo
  W! audit-existing-CSSL-source-files referencing-old-stub :
    grep "GEAR_RARITY_SYSTEM" → flag-as-needs-migration-to-canonical
    grep "ComponentRarity" → verify-uses-canonical-8-tier-enum (post-Q-06)
    grep "Affix" → verify-affixes-on-components-NOT-items
    grep "5-tier" + "6-tier" → migrate-to-8-tier (Q-06)
  W! ¬ block-current-work ← migration-can-be-incremental

---

# § 9 · FFI-SURFACE for-CSSL-source-author

## I> per-LoA-CSSL-source-thesis (foundational-feedback-LoA-v13-must-be-cssl-source)
  W! gear-modularity authored-in-.csl-source ¬ Rust-canonical
  Rust-stage-0 = compiler-output OR bootstrap-host ; ¬ canonical-source

## I> 100+-extern-C-symbols-needed (POD-3-blueprint-style)

### § 9.1 Coherence-Evaluator-FFI
```
extern "C" :
  // Per-dimension scorers
  fn coh_score_elemental_purity(item_handle: u64, out_score: *mut f32) -> i32
  fn coh_score_material_harmony(item_handle: u64, out_score: *mut f32) -> i32
  fn coh_score_cultural_coherence(item: u64, substrate: u64, out: *mut f32) -> i32
  fn coh_score_affix_synergy(item: u64, out_score: *mut f32) -> i32
  fn coh_score_biography_weight(bio_handle: u64, out_score: *mut f32) -> i32
  fn coh_score_paradox_tension(item: u64, substrate: u64, out: *mut f32) -> i32

  // Composite + Ascension check
  fn coh_evaluate_composite(item: u64, substrate: u64, out_eval: *mut CoherenceEvaluation) -> i32
  fn coh_check_ascension(item: u64, substrate: u64, trigger: u8, out: *mut AscensionCheck) -> i32
  fn coh_apply_ascension(item: u64, eval: *const CoherenceEvaluation) -> i32

  // Effect selection + magnitude
  fn coh_select_epithet_effects(item: u64, eval: *const CoherenceEvaluation,
                                 out_primary: *mut EpithetEffect,
                                 out_secondary: *mut EpithetEffect) -> i32
  fn coh_compute_effective_magnitude(ascension: u64, current_eval: *const CoherenceEvaluation,
                                      effect: *const EpithetEffect) -> f32
```

### § 9.2 Item-Composition-FFI
```
extern "C" :
  // Component CRUD
  fn item_create(form_id: u32, out_item: *mut u64) -> i32
  fn item_attach_component(item: u64, comp: u64) -> i32
  fn item_detach_component(item: u64, slot: u8) -> i32
  fn item_swap_component(item: u64, slot: u8, new_comp: u64) -> i32

  fn comp_create(comp_type: u8, material: u32, rarity: u8, volume: f32) -> u64
  fn comp_set_affix(comp: u64, slot: u8, affix: *const Affix) -> i32
  fn comp_get_durability(comp: u64) -> f32

  // Quality formula
  fn craft_compute_quality(material_hardness: f32, forge_level: f32,
                            player_affinity: f32, tool_quality: f32) -> f32
  fn craft_quality_to_tier(quality: f32) -> u8  // Crude · Common · Fine · Superior · Legendary
```

### § 9.3 Biography-FFI
```
extern "C" :
  fn bio_create() -> u64
  fn bio_log_event(bio: u64, event: *const BiographyEntry) -> i32
  fn bio_increment_kill(bio: u64, element: u8) -> i32
  fn bio_increment_biome(bio: u64, biome: u32) -> i32
  fn bio_get_total_events(bio: u64) -> u32
  fn bio_get_recent_entries(bio: u64, count: u32, out: *mut BiographyEntry) -> i32
```

### § 9.4 Affix-Interaction-FFI
```
extern "C" :
  fn affix_evaluate_all(item: u64, out_count: *mut u32, out_buf: *mut AffixInteraction) -> i32
  fn affix_check_chain(item: u64, element: u8, out_length: *mut u32) -> i32
  fn affix_check_conflict(item: u64, out_count: *mut u32) -> i32
```

### § 9.5 ComponentRarity + Resonance + Corruption-FFI
```
extern "C" :
  fn rarity_to_affix_slot_count(rarity: u8) -> u8  // 0,1,2,3,4
  fn comp_can_have_affix_slot(comp: u64, slot: u8) -> bool

  fn resonance_get_attunement(item: u64, element: u8) -> f32
  fn resonance_increment(item: u64, element: u8, delta: f32) -> i32
  fn resonance_apply_conflict_rules(item: u64) -> i32  // Fire suppresses Water · Void suppresses all

  fn corruption_get_value(item: u64) -> f32  // [-1.0, +1.0]
  fn corruption_shift(item: u64, delta: f32) -> i32  // Triggers Ascension check on threshold
```

## I> binding-from-cssl-source
  CSSL-source declares :
    extern "C" fn coh_evaluate_composite(item: u64, substrate: u64,
                                          out: *mut CoherenceEvaluation) → i32
  Stage-0-Rust provides : real-implementation @ compiler-rs/crates/cssl-substrate-coherence
  Future : CSSL-source becomes-canonical when KAN-substrate-runtime supports-evaluator-construction

---

# § 10 · OPEN-QUESTIONS for-Apocky-canonical-resolves

## I> Q-06 RARITY-TIERS · ✓ RESOLVED 2026-05-01
  verbatim : "Add mythic, prismatic, chaotic, in that order of ascending rarity."
  ⊑ resolution propagated into § 2.4 + § 6 (above)
  ⊑ 8-tier-canonical : Common · Uncommon · Rare · Epic · Legendary · Mythic · Prismatic · Chaotic
  ⊑ drop-curve : 60/25/10/4/0.9/0.09/0.009/0.001 (Σ = 100%)
  ⊑ glyph-slots : 0/0..1/1/1..2/2..3/3..4/4..5/5..6
  ⊑ ◐ sub-questions for-Apocky :
    Q-06a : 5×Epic→Legendary chain extends-to 5×Mythic→Prismatic ?
            recommend : NO (drop-only-tiers · preserves-rarity)
    Q-06b : Chaotic-affix-pool drawn-from-ALL-tiers OR Chaotic-tier-only ?
            recommend : ALL-tiers (wildcard-semantics matches-name)
    Q-06c : Σ-mask-randomization scope on-Chaotic affix-pool ?
            recommend : per-cell · sovereign-revocable

## I> Q-12 DRACONIC-ARCHETYPES · ✓ RESOLVED 2026-05-01
  verbatim : "Sovereign choice."
  ⊑ resolution : NO-canonical-mapping-forced @ archetype ↔ role
  ⊑ binding-matrix : 6 archetypes × 4 roles = 24-cells (sovereign-revocable)
  ⊑ 6 archetypes : Phantasia · EbonyMimic · Phoenix · Octarine · Paradox · Fossil
  ⊑ 4 roles : DM · GM · Collaborator · Coder
  ⊑ default fallback = Phantasia (archetype_id = 0)
  ⊑ NEW spec : Labyrinth of Apocalypse/systems/draconic_choice.csl



## OQ-1 : ω-field-cell-granularity for-13-axes
  Q? does-1-item map-to 1-omega-cell ?
     OR 1-item map-to 1-root-cell + N-component-children ?
  recommendation : 1-root + N-children ← matches-ItemComponent-tree
                 ; Σ-mask edges encode-component-attachment ; allows-hot-swap-as-edge-mutation
  W! Apocky-confirm before-implementing FFI-surface

## OQ-2 : LLM-generated-Epithet-names policy
  canonical : "LLM generates names and flavor only" «ARTIFACT_ASCENSION_SPEC:1725»
            ; "Epithet effects must be deterministic and balanced"
  Q? in-Mode-C-self-sufficient-default : Tier-2-LLM not-available
     do-we-fall-through to-Tier-1-template-only ? OR offer-local-LLM-via-cssl-llm-bridge ?
  recommendation : default Tier-1-template (already-canonical-fallback)
                 ; cssl-llm-bridge optional-Tier-2 when-host-has-credentials

## OQ-3 : substrate-regenerates-per-run vs-CSSLv3-coherence-data-isolation
  canonical : "Substrate regenerates per run" «ARTIFACT_ASCENSION_SPEC:1364»
  Q? in-multi-tenant-apocky.com architecture (per-project-Supabase-data-isolation)
     does-substrate live-in-LoA-Supabase-per-user ? OR @hub-Supabase shared-across-runs ?
  recommendation : per-LoA-Supabase per-user ← consent-default + sovereignty
                 ; cross-user-mycelial-sharing OPT-IN ¬ default

## OQ-4 : never-de-Ascend rule + Σ-Chain-immutability
  canonical : "items never de-Ascend" «ARTIFACT_ASCENSION_SPEC:1374»
  Q? AscensionState recording → Σ-Chain ?
     ⇒ AscensionState becomes-cryptographically-immutable not-just-game-rule-immutable
  recommendation : YES record-Ascension-events to-Σ-Chain
                 ; matches-Akashic-Records-design ; provides-cross-run-provenance

## OQ-5 : prosthetic-Ascension semantics
  canonical : "A prosthetic arm can Ascend." «ENTITY_BODY_SYSTEM_SPEC:27»
  Q? do-prosthetics use-same-13-axis evaluator ? OR specialized-evaluator ?
  recommendation : SAME-evaluator + add-axis-bonus :
                 ; Biography events for-NemesisKill-while-prosthetic-equipped
                 ; ParadoxTension boost for-cross-genre-prosthetic on-medieval-knight

## OQ-6 : pre-Ascended-loot 15%-chance vs-stage-0-fallback
  canonical : 15%-chance per-historical-item «ARTIFACT_ASCENSION_SPEC:1298»
  Q? in-stage-0-fallback (no-Substrate-history-yet) what-is-pre-Ascension-rate ?
  recommendation : 0% pre-Ascended ← can-only-Ascend-via-actual-play
                 ; stage-0 fallback = no-history → no-pre-Ascended-eligible

## OQ-7 : item-quality-score-tier-display
  canonical : "Coherence Score visible in tooltip = REJECTED" «ARTIFACT_ASCENSION_SPEC:1726»
            ; players-see Harmony-Dimension-indicators ¬ raw-number
  Q? what-UI-vocabulary for-the-6-Harmony-Dimensions ?
  recommendation : colored-bars + qualitative-labels per-dim
                 ; e.g., ElementalPurity = "Elemental: Devoted Fire-User"
                       ; AffixSynergyDepth = "Synergy: 3 chains active"
  W! Apocky-decide on-final-vocabulary

## OQ-8 : multiplayer-PvP-Ascension-visibility
  canonical : rejected "Ascension visible to other players as a PvP signal" «ARTIFACT_ASCENSION_SPEC:1724»
            ; "Revisit if PvP is formalized"
  Q? does-LoA-have-PvP ? if-no : irrelevant ; if-yes : need-revisit
  recommendation : LoA-currently PvE-co-op-default ← matches-Apocky-gift-economy-thesis
                 ; defer-PvP-question

## OQ-9 : Anomaly-RealityWarp safety
  canonical : "X% chance on hit to apply a random Chemistry Engine reaction" «ARTIFACT_ASCENSION_SPEC:295»
  Q? does-this-include catastrophic-BananaFlesh-failsafe ?
     ⇒ random-chemistry-could-banana-flesh-the-target's-equipment !
  recommendation : Anomaly excludes-BananaFlesh ← keeps-rarity-of-failsafe
                 ; explicit-catastrophic-failure remains alchemy-only

## OQ-10 : LoA-content-pack monetization-vs-cosmetic-only-axiom
  canonical-feedback : "cosmetic-only-axiom · ¬ pay-for-power" (LoA-legacy-design)
  Q? Ascension is-power-bonus (~15-25%) ; can-Apocky monetize Pre-Ascended-artifacts ?
  recommendation : ¬ monetize Pre-Ascended-power
                 ; CAN-monetize cosmetic-Epithet-visual-overlays (glow-hue · particle-style)
                 ; never-Pay-to-Ascend ; only-Pay-to-Style-already-Ascended

---

# § 11 · CANONICAL-QUOTES-INDEX (≥10 with-LOC-from-source)

## I> ARTIFACT_ASCENSION_SPEC.md citations
  Q1 «:8» : "This system must NEVER introduce randomness into the Ascension evaluation.
            Coherence is deterministic. Given identical item state, the score and Epithet
            are always identical."

  Q2 «:18» : "The player builds the instrument. The game recognizes the symphony."

  Q3 «:46-47» : "Axis 13 is the capstone. It reads axes 1–12 and, if the item crosses
                the Ascension threshold, generates an Epithet — a permanent named bonus
                derived from the item's dominant characteristics."

  Q4 «:830» : "public const float AscensionThreshold = 70.0f;"

  Q5 «:1370» : "The Ascension threshold is tuned so that maxing any two dimensions doesn't
              automatically clear 70."

  Q6 «:1374-1376» : "Items never de-Ascend ... If the player later swaps a component
                    that reduces the theoretical Coherence Score below threshold,
                    the Epithet persists — the item 'remembers' what it was."

## I> CHEMISTRY_MAGIC_CRAFTING_BIBLE.md citations
  Q7 «:524» : "Why affixes on components, not items: A Common Gold Crossguard is just
              gold stats. A Rare Gold Crossguard has 2 rolled magical properties — same
              material, different rarity, completely different gameplay value."

  Q8 «:526» : "Components unlock at quality thresholds — Guard@q2, Pommel@q3, Edge@q5,
              Fuller@q7. Higher quality items have MORE component slots, not just better
              base stats."

  Q9 «:577» : "Nothing is truly worthless."

  Q10 «:827» : "Affixes live on individual components, driven by component rarity. This was
                a deliberate Phase M decision — component-level affixes create exponentially
                more variety than item-level ones."

## I> ENTITY_BODY_SYSTEM_SPEC.md citations
  Q11 «:8» : "Bodies are NOT stat blocks with visual skins. Bodies are physical objects
            made of materials."

  Q12 «:27» : "A prosthetic arm can Ascend."

## I> Master-Plan.md citation
  Q13 «:9» : "Everything is Material. Inspired by IVAN, items and creatures are
            compositions of form and substance. A sword made of bread behaves like bread."

---

# § 12 · COMPLETION-GATE

## ✓ DONE — this-spec
  ✓ HISTORICAL-CONTEXT explicit
  ✓ 13-axes per-axis-formula + LOC-cross-reference
  ✓ 6-dim-Harmony per-dim-formula + scoring-table
  ✓ Ascension-pipeline deterministic + magnitude-scaling-canonical
  ✓ Item-Biography ring-buffer-design canonical
  ✓ Component-Rarity-Affix-System full-canonical
  ✓ ω-field-cell INTEGRATION-PROPOSAL
  ✓ CSSL3-stub DEPRECATION-strategy (compose ¬ replace)
  ✓ FFI-SURFACE 100+-symbols (Coherence + Item + Biography + Affix + Rarity + Resonance + Corruption)
  ✓ OPEN-QUESTIONS 10× for-Apocky-resolution
  ✓ canonical-quotes-index ≥10 with-LOC

## I> next-steps (post-Apocky-canonical-resolves)
  W14-J : implement-stage-0-Rust crate cssl-substrate-coherence (per-§9-FFI)
  W14-K : author-CSSL-source bindings @ specs/infinity-engine/coherence/*.csl
  W14-L : audit-existing-CSSL referencing-old-CSSL3-stub → migrate-to-canonical
  W14-M : compose-cosmetic-overlays from-CSSL3-stub onto-canonical-rarity-tiers

# § fin
