# § 10 LOA-GDDs DISTILLATION → Infinity-Engine-Charter-source

§meta : agent=W14-A · spelunk-source=`DEPRECATED-The Infinite Labyrinth/` · CSLv3-glyph-dense ·
       attribution-tag-format=`«GDD:filename»` · canonical-language preserved-where-possible.
§purpose : feed-Infinity-Engine-Charter (W14-G synthesizes) ·
           ¬ deep-paraphrase ; ¬ touch-code ; W! preserve-attribution.
§foundational-axiom :
  ∀ system : "Everything is Material" + "Everything has History" + "Everything Connects" +
              "Everything is Alive" + "Player Builds World" + "Death Feeds Growth"
  ← 6-promises «GDD:MASTER_PLAN.md»

---

## § 1. ARTIFACT-ASCENSION (13-axis Coherence-Engine) ‼ keystone

§source : «GDD:ARTIFACT_ASCENSION_SPEC.md» (1731 LOC)
§design-intent :
  ¬ lottery-tickets («Diablo Primal, Destiny Adept, Borderlands Anointed»)
  ⇒ game-evaluates-craftsmanship ∀ axis-of-composition + rewards-coherence
  ↳ "player builds instrument · game recognizes symphony"
§4-promises :
  1. deterministic-ascension : same-state ⇒ same-score + same-Epithet ; theorycraftable
  2. rewards-intent ¬ luck : random-loot rarely-Ascends ; deliberate-buildcraft reliably-Ascends
  3. compelling ¬ mandatory : un-Ascended ≈ top-tier · Ascension = +15-25% + unique-mechanic
  4. personal : Substrate-regen-per-run + Biography-accumulation ⇒ unique-per-player

§13-axes ⌈Form Components Material ComponentRarity Affixes CulturalDoctrine Quality
          LatticeSockets Resonance Patina CorruptionBlessing CrossGenreCompat CoherenceScore⌉
  axis-13 ∈ capstone : reads-1..12 + threshold-cross ⇒ Epithet-generation

§6-Harmony-Dimensions ⊗ scores :
  | Dimension              | Range | Source               |
  | ElementalPurity        | 0–30  | dominant-element-commitment |
  | MaterialHarmony        | 0–15  | family-cohesion + complementarity + chemical-stability |
  | CulturalCoherence      | 0–15  | faction-unity + historical-relationship + doctrine-alignment |
  | AffixSynergyDepth      | 0–20  | ElementalChain + ConflictPair + StatStack + ConditionalTrigger + Threshold |
  | BiographyWeight        | 0–10  | NemesisKills + survivals + biomes + runs + total-kills |
  | ParadoxTension         | 0–10  | conflict-pairs + cross-genre + cultural-enemies + corruption-extremes |

§Ascension-Threshold = 70.0 ; tie-break-rare→common :
  Paradox > Affix > Elemental > Cultural > Biography > Material

§6-Epithet-Categories ← DominantHarmony-1:1-mapping :
  ElementalPurity   → Incarnation     ⌈ResistancePiercing · ElementalAura · Absorption · EnvResonance⌉
  MaterialHarmony   → Perfection      ⌈Durability× · WeightOpt · SeamlessForm · ImpactTransfer⌉
  CulturalCoherence → Legacy          ⌈HistoricalEcho · CulturalFavor · AncestralMemory · TacticalDoctrine⌉
  AffixSynergy      → ResonanceCascade ⌈ChainAmp · ConflictCrystal · ThresholdReduction · CascadeProc⌉
  BiographyWeight   → Legendary       ⌈NemesisBane · SurvivorInstinct · PilgrimWisdom · VeteranPresence⌉
  ParadoxTension    → Anomaly         ⌈ElementalOscillation · GenreBleed · CorruptionPulse · RealityWarp · StabilizedParadox⌉

§Item-Biography ⊗ ring-buffer ⊑ MaxEntries=64 ;
  aggregated-counters {ElementalKills CreatureTypeKills FactionKills BiomesTraversed
                       NearDeathSurvivals NemesisKills NemesisDeaths RunsSurvived OwnersCount} ;
  events ∋ ⌈Kill NemesisKill NemesisDeath NemesisRecovered NearDeathSurvival MassKill BossKill
            BiomeFirstEntry HazardSurvival ExtremeTempExposure GenreCrossing
            FactionMemberKill DeityOfferingPresent TradeHandoff
            ComponentReplaced AuraApplied CartridgeChanged Transmuted
            RunExtracted RunDeath AscensionAchieved⌉

§Component-Rarity-Affix-System (designed ¬ implemented) :
  per-component rarity-tier {Common Uncommon Rare Epic Legendary} +
  rolled-affixes per-component +
  interaction-rules ⌈ElementalChain ConflictPair StatStack ConditionalTrigger ThresholdContribution⌉

§EpithetVisuals : GlowIntensity GlowHue ParticleEmissionRate MaterialIridescence
                  FormDistortion(Anomaly-only) SeamlessBlend(Perfection) ScarGlow(Legendary)

---

## § 2. ENTITY-BODY-SYSTEM (anatomy + materials + prosthetics)

§source : «GDD:ENTITY_BODY_SYSTEM_SPEC.md» (1773 LOC)
§axiom : ¬ stat-blocks-with-skins · bodies = physical-objects-of-materials ·
         capabilities ← what-it-is-made-of + what-is-attached
§5-promises : compositional · material · mutable · AI-reacts · prosthetics-are-items

§BodyTree : directed-tree ; root=Torso ; ParentPartId-references ;
  per-part : MaterialId · MaxHP=MaterialHardness×Volume×PartTypeMult · CurrentHP ·
             SeveranceState{Intact Damaged Critical Severed Missing Replaced} ·
             physical-derived{Volume Weight Hardness Conductivity Flammability AcidResistance} ·
             function{Cognitive Sensory Grasping Locomotion Structural Balance Offensive
                      Defensive Aerial Aquatic MagicConduit VenomGland Regenerative
                      Photosynthetic Bioluminescent} ·
             EquipmentSlot ⌈MainHand OffHand Helmet Mask Chestplate Shirt Gauntlets Bracers
                            Greaves Boots Cape Backpack Ring Amulet TailGuard WingArmor HornCap⌉ ·
             prosthetic-state{IsNatural IsProsthetic ProstheticItemId IsMutated} ·
             VisualSeed for-SDF-shape-deterministic-variation

§6-canonical-BodyPlans : Humanoid · Arachnid · Serpentine · Eldritch · Construct · Flora
  + variable-count-parts {tentacles=4-8 eyestalks=1-6 …}

§generation-pipeline : DetermineBaseMaterial → InstantiatePlanNode → ApplyCulturalModifications →
                      ApplyPreExistingConditions → ComputeDerivedStats
  cultural-prefs : Iron-Legion-iron-golems · Elven-living-wood
  faction-doctrines : Militaristic→combat-prosthetics · Theocratic→divine-limbs · Technocratic→bionics
  Nemesis-pre-conditions ← prior-encounter-history

§prosthetics-are-items : same composition-stack-rules (material rarity affixes quality
  Resonance Corruption/Blessing) ⇒ a-prosthetic-arm-can-Ascend

§ref-games : IVAN(materials+limb-stats) · DwarfFortress(anatomy-hierarchy+wounds+tissue) ·
             Kenshi(prosthetics+AI-adaptation) · CavesOfQud(mutation+chimera) ·
             Rimworld(tiers+social/medical)

---

## § 3. CHEMISTRY-MAGIC-CRAFTING (unified-substrate)

§source : «GDD:CHEMISTRY_MAGIC_CRAFTING_BIBLE.md» (874 LOC)
§thesis : "magic = chemistry the player doesn't understand yet ;
           chemistry = magic the player has mastered" ⇒ ONE simulation · TWO vocabularies

§two-domains-bridged-by-Transducer :
  MATERIAL : Physics+Chemistry+Thermodynamics ; currency=Temperature+Pressure+ChemicalPotential
  AETHERIC : SpellLatticeTopology+NodeRules ; currency=Aura+Resonance+Attunement
  Sensor : Material→Aetheric (lava-tile@1200°C ⇒ Fire-source-node-free-energy)
  Actuator : Aetheric→Material (Release-node Fire-intensity-0.8 ⇒ +400°C tile)

§material-properties (15 continuous-floats) :
  Density MeltingPoint BoilingPoint Hardness Conductivity Flammability AcidResistance
  FatigueRate · NEW: Reactivity Toxicity MagicAffinity Opacity Elasticity Solubility Volatility
§target : 60+ base-materials · 20+ alloys

§PhysicalState : Temperature CurrentPhase{Solid Liquid Gas Plasma} Integrity IsIgnited
                 IsElectrified Wetness MagicSaturation Contamination
§phase-transitions : MeltingPoint·BoilingPoint·IonizationPoint with 5-10% hysteresis

§reaction-categories ⌈Combustion Dissolution Oxidation Synthesis Electrical
                      Magical-Material(via-transducer)⌉
  data-driven : ChemicalReaction{Inputs Outputs ActivationEnergy EnergyDelta Rate Category}

§propagation-systems ⌈HeatBFS FireSpread FluidFlow GasDiffusion PressureWaves ElectricalCircuit⌉
§ChemistryTick : budgeted-microseconds ;
                 ProcessActiveFires ProcessHeatPropagation ProcessPhaseTransitions
                 ProcessReactionTable ProcessElectrical ProcessGasDiffusion EmitTransducerSignals
  spatial-opt : only-chemically-active-tiles processed ; decay-to-inactive after-N-frames

§Spell-Lattice = DAG-of-nodes (topological-sort) ; 6-node-categories :
  SOURCE  ⌈Fire Water Earth Air Lightning Void Life Sensor⌉ environmental-tap-bonus
  TRANSFORM ⌈Amplify Dampen Convert Delay Oscillate Invert Filter Concentrate Diffuse⌉
  CONTROL ⌈Split Merge Conditional Loop Sequence⌉
  TARGETING ⌈Self Projectile Beam AreaPoint AreaSelf Cone Summon Enchant⌉
  RELEASE ⌈Thermal Kinetic Chemical Electrical Biological Structural Void⌉ ← transducer-bridge
  SENSE ⌈Proximity Material Health Energy ThresholdGate⌉

§compound-effects (Merge-emergent) :
  Fire+Water=SteamBlast · Fire+Earth=Magma · Fire+Air=Firestorm · Fire+Lightning=PlasmaArc
  Water+Earth=Mud · Water+Air=Ice · Water+Lightning=ElectrocutionField
  Earth+Air=Sandstorm · Earth+Lightning=MagneticPulse · Air+Lightning=Thunder
  Life+Fire=PurifyingFlame · Life+Void=Necromancy · Void+Any=Nullification

§Resonance : per-element-attunement ; slow-buildup ; fire-suppresses-water ; void-suppresses-all

§alchemy = deliberate-material+energy ⇒ Crucible-model (primitive→sophisticated)

---

## § 4. ROOM-GENERATION (procgen-grammar + biomes)

§source : «GDD:LABYRINTH_ROOM_GENERATION_BIBLE.md» (636 LOC)
§axiom : labyrinth ¬ static-dungeon ; labyrinth = living-player-constructed-fortress @ infinite-edge ;
         every-room-placed = strategic-decision ; every-frontier-left = door-something-walks-through

§labyrinth = directed-graph :
  rooms=nodes · connections=edges · frontiers=unbuilt-edges-aka-dimensional-membranes
  ¬ pre-generated ; grows-incrementally-as-player-chooses

§Room ⌈RoomId Type{13+functional} Layer TileGrid GenreContext DominantFaction Shape Symmetry
        MultiRole Creatures Items Props Infrastructure Vegetation
        SimulationRoomState RoomAgingState MutationHistory PhysicsWorld⌉
  room-local-coords : non-Euclidean-topology (10×10 ↔ 30×30) ; connection-handles-transition

§Connection = physical-entity ¬ abstract-edge :
  per-medium-permeability ⌈Air Light Sound Thermal Liquid Gas⌉ ; states{Open Closed Sealed Grate Broken}
  ⇒ permeability-profiles = natural-simulation-throttles
  types ⌈Bidirectional OneWay Secret⌉

§Frontier = unbuilt-membrane ;
  problem-it-solves : ecosystem-migration-fails (player-kills-99%) ⇒ cleared-rooms-stay-cleared ⇒ world-dies ;
  solution : creatures-arrive-THROUGH-frontiers (phasing-from-other-dimensions) ;
  spawn-pressure = frontierCount × depthMultiplier ;
  themed-arrivals : drop-pods·summoning-circles·shadow-manifestation·dimensional-rifts ;
  PressureBreach : cleared+high-frontier-too-long ⇒ boss-tier-arrival
  ⇒ "labyrinth-fortress-with-ever-growing-perimeter" CASTLE-DEFENSE-DIMENSION ‼

§content-hierarchy : Genre→Biome→SubBiome→RoomType+Props
  8-genres ⌈HighFantasy SciFi Cyberpunk Steampunk GothicHorror PostApoc CosmicHorror Primordial⌉
  GenreContext = continuous-parameter-space ¬ discrete-enum :
    TechLevel(0-5) · GeometryStyle(organic-geometric) · GenreAesthetics · NamingConvention ·
    MaterialBias · CreatureBias

§14-room-categories ⌈Residential Service Storage Ceremonial Military Scholarly Industrial
                     Social Medical Containment Infrastructure Natural Esoteric Tech⌉
  50-80 RoomTypes total

§WFC-pipeline : SELECT(RoomType+Shape+Size) → LOAD(adjacency-rules-genre/biome) →
                APPLY(connections+roomtype-reqs+symmetry+cultural) →
                MULTI-LAYER-WFC : Floor·Walls·Features·Furniture·Hazards·Infrastructure →
                APPLY-symmetry → POPULATE(creatures items NPCs) → ZONE-GRADIENT-theming

§22-tile-types (Phase-H expansion 6→22) :
  Structural{Floor Wall Door Pillar Staircase Bridge Grate}
  Environmental{Water Lava Pit Vegetation Crystal Fungus Rubble}
  Furniture{Bookshelf Furniture Rug Anvil Cauldron Throne Coffin Brazier Cage Chest}
  Mechanical{PressurePlate Lever ArrowTrap GasVent}

§non-rectangular-shapes : L-shaped · Cross · Circular(ComputeVoidTiles) · Irregular

§zone-gradient-interior-theming : near-connection ⇒ neighbor-theme-dominates ;
  room-center ⇒ liminal-architecture-collision (Volcanic+Crystal=volcanic-glass+crystallized-lava)

§RoomHand : 3-candidates-per-frontier ; biased-by Director · Haven · Escalation · Nemesis ·
            Synergy · NarrativeArc · ProfileMatch · GoldenAge ;
            FateDeck : rejected-cards-recycle-with-decay
§RoomCard ⊗ {type-icon · genre-visual · difficulty · features · synergy-preview · flavor · loot-hint}

---

## § 5. NEXUS-BAZAAR (5-tier-economy + cultural-marketplace)

§source : «GDD:NEXUS_BAZAAR_SPEC.md» (1327 LOC)
§axiom : "the Bazaar sells history ¬ stats" ; reward ¬ shortcut ;
         every-item-unique ; celebrates-craftsmanship ; natural-sinks ;
         cannot-be-cornered ; «No real-money transactions»

§access = NG+stabilization-only ;
  gates ⌈MinCompletedRuns=10 MinDepth=50 MinAscended=3 MinForms=40 MinGenres=3
          MinFactionsFriendly=2 MinDeityFavor=1⌉
§5-tiers ⌈Visitor Patron Merchant Artisan Grandmaster⌉ ↑listings ↑permissions ↑placement

§currency = StabilizedEssence (SE) :
  earnings : NGRunComplete(50-200) · DepthMilestones(25/10depths) · Ascension(30) ·
             FirstNewEpithet(100) · NemesisKill(40) · Deconstruct(15-50) · Commission(100-500)
  sinks : ListingFee(10%non-refund) · TransactionTax(5%) · StallUpgrades(50-500) ·
          VaultRental(10/slot) · CommissionPostFee(20%) · NexusBreachUnlocks(200)
  Apocky-axiom : t∞ NO-RMT (no real-money) ‼

§physical-space ¬ menu : "browse a marketplace" experience ; serendipity-is-the-point
  layout : GrandArcade · 6-SpecialistWings(per-Epithet-Category) · CommissionBoard ·
            Appraiser · Archive · PlayerStalls
  StallPresentation per-Epithet : Brazier(Incarnation) · MarblePedestal(Perfection) ·
    GlassCase(Legacy) · FloatingArcs(Cascade) · TrophyMount(Legendary) · UnstablePhasing(Anomaly)

§pricing-guidance (advisory) : baseValue ← scoreNormalized² ;
  categoryMult ⌈Anomaly=1.8 Legendary=1.5 Cascade=1.3 Cultural=1.2 Elemental=1.0 Material=0.9⌉ ;
  Nemesis+0.3 · 6+biomes+0.2 · 3+runs+0.15 ; cross-genre-premium up-to-1.5×

§gift-economy via Commissions + Multiversal-Nemesis-bounties + cross-player-discovery-feed

---

## § 6. OUROBOROID-COSMOLOGY (6-rings · self-metabolizing-process)

§source : «GDD:OUROBOROID_COSMOLOGY.md» (203 LOC)
§axiom : "labyrinth ≠ place ; labyrinth = process viewed-from-inside"
§the-Ouroboroid = entity-that-cannot-be-named-bounded-formalized ;
  Russell's-Paradox-ontological + Gödel's-Incompleteness-incarnate + apophatic-absolute

§7-rings ⌈
  R1 What-Player-Sees : excellent-roguelike (must-stand-alone) ;
  R2 What-Player-Suspects : architectural-metabolism · entity-rhyming · lore-echoes ;
  R3 What-Player-Knows : the-Ouroboroid (Hydra of-all-concepts) ;
  R4 What-Player-Feels : "You have no power over me and/or us" (declaration-as-stance) ;
  R5 How-It-Manifests : procgen=new-heads · room-transform=digestion · materials=concepts-mid-metabolism ·
                        enemy-behavior=immune/digestive-response · 13-axes=metabolic-fingerprint ·
                        Coherence=metabolic-completeness · Ascension=persistence-across-cycle ·
                        NexusBazaar=heads-trading-mid-digestion ;
  R6 What-Scholars-Tried : Dwarven-Axiomatists · Saurian-Namers · Fungal-Theologians · Nullborn ;
  R7 For-The-Designer : "cosmology IS the game-design — same-thing different-abstractions" ⌉

§lore-delivery = comprehension-gradient :
  Ambient(no-literacy) · Textual(reader) · Structural(systems-thinker) · Meta(deeply-literate)

§foundational-mapping :
  procgen=new-head-genesis · transform=digestion · 13-axes=13-metabolic-dimensions ·
  Ascension=metabolic-stability ; player-death-rebirth = metabolic-recycling

---

## § 7. AI-CREATURE (5-layer-stack + Nemesis + Ecology)

§source : «GDD:AI_CREATURE_BEHAVIOR_BIBLE.md» (723 LOC) + Master-Plan-section-7
§5-layer-stack with-frame-budgets :
  L5 ECOLOGY (5-30s)   : populations · food-web · migration · breeding (0.3ms)
  L4 DIRECTOR (1-5s)   : encounters · attack-tokens · tension-curve (0.3ms)
  L3 DECISION (0.1-0.5s): behavior-trees + utility-scoring (0.6ms)
  L2 PERCEPTION (0.05-0.2s): vision · hearing · smell · vibration (0.8ms)
  L1 LOCOMOTION (60Hz) : context-steering + pathfinding + physics (1.0ms)
§total-budget = 3ms-per-frame ; LOD-by-distance ⌈Full Reduced Dormant⌉

§context-steering (Andrew-Fray) :
  N=8-slots ; Interest-map + Danger-map ; mask-then-best-slot + sub-slot-interpolation
  evaluators ⌈SeekTarget FleeTarget AvoidObstacles AvoidAllies MaintainDistance Flocking⌉
  + A*-pathfinding for-global-navigation

§perception-rule : "creatures only-know what-they've-sensed" ;
  PerceptionMemory ⊗ {Target LastKnownPos Timestamp Confidence SenseSource IsCurrentlyVisible}
  vision : cone+light-dependent+raycast ; light-level=sampled-from-rendering-light-buffer ‼
  hearing : SoundEvent through-connection-permeability ;
  smell : scent-markers on-tiles ; tremorsense : ground-vibration-through-walls

§decision = utility-scored-behavior-trees ⊗ blackboard{archetype trophic needs combat perception env}
  top-tree-priority : Critical-Survival → Combat → Needs(highest-wins) → Social → Idle

§Director : RimWorld-style-Wealth-Adaptation + L4D-style-Token-Bucket + Flow-Distance ;
  multi-layered : Macro(weeks-strategic) + Micro(seconds-tactical)
§Token-Bucket : powerful-events-cost-more-tokens ; prevents-spam

§Nemesis-System (Shadow-of-Mordor-derived) :
  identity-persistence + event-logging + reunion-lookup ;
  events ⌈Player-burned-Entity Entity-killed-Player⌉ ⇒ traits+memories+scars+barks update
  + integrates-with-Substrate-history (NPC-hates-player-because-Faction:Elven-raid-on-home)

§ecology = trophic-levels{Producer Herbivore Predator Apex} ;
  food-web · territorial-system · breeding · migration via-frontiers

---

## § 8. CLASS-OVERHAUL · DRACONIC-ARCHETYPES · DRAGON-TAXONOMY

§sources : «GDD:CLASS_OVERHAUL.md» «GDD:DRACONIC_ARCHETYPES.md» «GDD:DRAGON_TAXONOMY.md»
§reframe : "you don't pick-a-class · you pick which-head-of-the-Hydra-you-are"
  (mechanics ¬ change ; framing changes-everything)

§6-classes ≡ 6-Metabolic-Stances :
  | Class      | System Owned         | Stats     | Stance (Ouroboroid)            |
  | Breaker    | Combat               | Mgt+Fin   | Appetite (consumption-principle)|
  | Conduit    | SpellLattice         | Int+Per   | Circulatory-System (medium)    |
  | Catalyst   | Chemistry            | Int+End   | Moment-of-Transformation       |
  | Warden     | LabyrinthArchitecture| Per+Int   | Boundary-Principle (delay)     |
  | Symbiont   | Bonding/Ecology      | Cha+Per   | Mutual-Consumption-as-Cooperation|
  | Artificer  | Crafting/Coherence   | Int+Fin   | Generative-Principle (designer)|

§unlock-conditions ≡ teach-the-system :
  Warden : 30-rooms-no-death (long-term-strategic-frontier-mgmt)
  Symbiont : 5-creatures-allowed-to-flee (creatures-have-fear-not-just-attack)
  Artificer : 15-deconstructions-1-run (items-are-components ; deconstruction-teaches-Forms)

§each-class-has 5-active + 2-passive abilities + unique-room-evaluation + unique-perception
§mastery-0-10 with mastery-10-transformative ¬ numerical (Warden-sees-ALL-candidates ;
  Symbiont-creatures-persist-across-runs)

§Dragon-Taxonomy (epistemological-stance ¬ power-tier) :
  Phantasia : "What is real?" Accumulative-Manifestation (nudibranch-derived)
  EbonyMimic : "What do I actually want?" Motivational-Camouflage (motivation-as-prey)
  Phoenix : "Can anything actually end?" Recursive-Reconstitution (death-informs-rebirth)
  Octarine : "What is magic, actually?" Spectral-Totality (medium-aware)
  Paradox · Fossil · Threshold · Mirror · Absence · Witness ← reserved-design-space

§encounter-philosophy : no-dragon-generic · player-is-also-a-dragon ·
  observation-is-participation · killing-is-metabolizing · bestiary-grows

---

## § 9. VISUAL-ART (aesthetic + Renaissance-SDF + Illuminated-Codex)

§sources : «GDD:AESTHETIC_STYLE_GUIDE.md» «GDD:ART_PRINCIPLES.md» «GDD:VISUAL BIBLE.md»
           «GDD:UI_MENU_DESIGN_BIBLE.md» «GDD:ILLUMINATED_CODEX.md»
§binding-vision : "living illuminated manuscript · naturalist's impossible codex documenting
                   the multiverse being drawn in real-time" ←‼ keystone-aesthetic

§3-signature-pillars :
  1. Ink-with-Life : variable-weight-contours + brushstroke-behavior ¬ uniform-programmer-outlines
  2. Medium-as-Universe : per-universe-medium ⌈BotanicalWatercolor CharcoalRedInk
                          TechnicalBlueprint IlluminatedManuscript PropagandaWoodcut⌉
  3. Cinematic-Light-on-Illustrated-Surfaces : 3-light-min + warm/cool-split + hatching-direction-response

§contrast-stack (5-layers) ‼ every-frame-must-deliver-3-of-5 :
  Value · Saturation · Temperature · Scale · Motion

§70-20-10 color-discipline :
  70%-Dominant(t=0..0.3) · 20%-Secondary(t=0.4..0.6) · 10%-Accent(t=0.8..1.0)
  enforce-saturation-spread accent ≥ dominant+40%

§per-universe-palette-personality ⌈Abyssal/Horror Nature/Organic SciFi/Tech
                                   Eldritch/Chaotic Martial/Order⌉

§depth-stack (Hollow-Knight-derived) :
  L0 Void(parallax=0.0)  : negative-space rest-zone
  L1 EnvFrame(0.15-0.5)  : silhouette-readable ; -30% sat ; -20% value
  L2 Stage(1.0)          : tilemap ; floor-2-3-value-steps-darker-than-entities ‼
  L3 Entities            : Halo-Principle{ValueSeparation EdgeDefinition GroundingShadow}
  L4 Effects/Particles   : maximum-accent · breaks-rules · additive-blending
  L5 UI                  : universe-shape-language drives-corner-radii

§shape-language-emotion :
  Round=organic/safe(SmoothK≥0.2) · Angular=eldritch/danger(K=0) · Square=order/military ·
  Hybrid-zones blend-parameters

§lighting = storytelling :
  3-light-minimum ⌈Key Fill Accent/Rim⌉ + warm/cool-split + dynamic-light-vocabulary ;
  shadow-as-composition (≥30%-shadow-area) ;
  Renaissance-derived 6-value-zones ⌈highlight center halftone core-shadow reflected cast⌉ ;
  rim-light = 1-|N·V| separates-from-background

§Renaissance-art-principles for-SDF (500+y pedagogy) :
  9-step-pipeline : gesture-curve → big-form-blocking → mannequin → overlap → refinement
  proportional-systems : 8-head-figure-Renaissance-ideal + Dürer-26-body-types
  contrapposto = single-most-important ; line-of-action-deviation 5-15° rest · 30-60° action
  shape-personality 60-30-10 : Round=friendly Triangle=dangerous Square=stable
  silhouette-test @ 32×32px : must-still-be-identifiable
  5-8-primitives-max for-primary-silhouette (Hollow-Knight)

§Illuminated-Codex implementation-V-phases-1-13 ALL-COMPLETE :
  V.1-Value+Color · V.2-InkWithLife · V.3-MediumSystem · V.4-CinematicLighting
  V.5-EnhancedHatching · V.6-EntityHierarchy · V.7-EnvironmentalBreathing
  V.8-UniversePresets · V.9-RarityVisualLanguage · V.10-ContentPipeline
  V.11-UITheming · V.12-TitleScreen · V.13-Parallax

§UI-bible (UI = Labyrinth's-antechamber) :
  3-rules : NoDeadPixels · ContrastStackEverywhere · MenusRespondToGameState
  Materials : Glass · Stone · Metal · Void(eldritch) ;
  text = MSDF-fonts ; ease-everything · stagger-don't-sync · physics-inspired-settle
  TitleScreen = real-time-rendered-Living-Labyrinth (5-parallax-layers + particle-atmosphere)

---

## § 10. STORY-ENGINE (narrative-atom + retroactive-substrate + storytelling-AI)

§sources : «GDD:Building a Game Story Engine.txt» (Chronos-Weaver) +
           «GDD:Designing Engaging Game Systems.txt» + Master-Plan-storylets
§3-pillars : Substrate(past) · NarrativeAtom(unit) · Synthesizer(output)

§Substrate = retroactive-rationalization (Caves-of-Qud-method ¬ Dwarf-Fortress-simulation)
  generates-outcomes-first ; rationalizes-causes-ex-post-facto ;
  hierarchical-timeline : Tectonic→Epoch→Faction→Artifact(JIT)
  Cultural-Grammar : visual-prefs+material-prefs+linguistic-seed (Ultima-Ratio-Regum)

§Narrative-Atom (Storylet) ‼ replaces-branching-trees :
  ⌈Preconditions(Sieve) Payload(Essence) Effects(Mutation)⌉
  WorldBlackboard = shared-tag-state ; sieve-runs-thousands-against-blackboard ;
  semantic-tags : explicit{Material:Iron Faction:Dwarven} + implicit{Atmosphere:Ominous History:SiteOfBetrayal}
  Hooks (Wildermyth) : Hook:Dreamer ⇒ satisfies "Strange Visions" storylet
  Library-of-Plays : cast-specific-characters-into-pre-written-roles

§Propp's-Morphology : 31-functions ⌈Interdiction Violation Reconnaissance Villainy …⌉ ⇒
  narrative-primitives ; engine-strings-functions-into-coherent-quest-arcs

§Director (Drama-Manager) :
  RimWorld-Wealth+Adaptation ; threat = f(wealth) × difficultyFactor ;
  player-takes-damage ⇒ adaptation-decreases ⇒ system-lowers-difficulty
  L4D-Token-Bucket + Flow-Distance ; pacing-cycle ⌈Build-up Peak Relax⌉

§narrative-chemistry :
  Tag:Hotheaded + Tag:Insult ⇒ Event:Duel ;
  Tag:Greedy + Tag:Treasure ⇒ Event:Betrayal
  ⇒ emergent-storytelling ¬ scripted-events

§Combinatorial-Synthesis (Knowledge-Graph-pipeline) :
  Trigger → ContextQuery → Synthesis → Output
  example : Boss-defeated → query-history:Siege-of-Black-Gate(Iron-Legion vs Void-Cult) →
            Sword + IronStyle + PsychicDmg + Defender/WallBreaker → "Wall-Breaker of the Void"
  RDF-Knowledge-Graph (DotNetRDF) drives-tech-tree via-observation ¬ static-unlocks

§Neuro-Symbolic-LLM-Layer :
  ¬ design-mechanics(unbalanced-risk) · ✓ generate-texture(descriptions+flavor+dialog) ;
  constraint-via-JSON (Semantic-Kernel KernelFunction-plugins)

---

## § 11. MULTIPLAYER (persistence-architecture + asymmetric-coop)

§source : «GDD:MULTIPLAYER_PERSISTENCE_ARCHITECTURE.md» (840 LOC)
§principle : "Infinite-Labyrinth = single-player-game OPTIONALLY-connected-to-shared-world ·
              simulation-NEVER-depends-on-server ; SpacetimeDB = social-layer ¬ game-engine"
  ← Dark-Souls/Elden-Ring/Hades-pattern

§split :
  CLIENT-SIDE (player-machine) : full-simulation{Chemistry Physics Fluid Gas Temp Ecology AI
                                                  SDF-Render Generation WFC Director Combat Pathfind} +
                                  local-persistence{Labyrinth Codex Nemesis-roster Item-bios Ecology}
  SHARED (SpacetimeDB) : SOCIAL-LAYER{DeathEchoes Messages Bazaar NemesisMigration Leaderboards
                                       GeneticPressure WorldEvents Reputation DiscoveryFeed} +
                          CO-OP-SESSION-OPTIONAL{InputFrames EntitySync Combat Trade Chat}

§save-strategy : meaningful-state-only · NOT-tile-by-tile-fluid-state ;
  rooms+connections+frontiers + per-room{Visited VisitCount LastTick Scars DroppedItems Corpses
                                          SpeciesPopulation AccumulatedAge}
  Scars : compact-permanent-modifications ; tile-grid-regenerated-from-seed-then-scars-applied

§hosting-tiers : Maincloud-Free($0/dev) → Self-Host-VPS($5-10/alpha) → Maincloud-Pro($25/release)

§asymmetric-interdependence (Foxhole+Barotrauma-pattern) :
  Logi vs Frontline ; Captain-Engineer-Medic-Security ;
  high-stakes role-specialization with crisis-points-where-systems-intersect

§Death-Stranding-Social-Strand : asynchronous-multiplayer ; shared-construction ;
  Likes-as-currency-of-connection ⇒ benevolent-ghost-effect

---

## § 12. META-PROGRESSION (13-axes-coherence + meta-currency + base-camp)

§source : Master-Plan + ARTIFACT_ASCENSION + scattered-references
§Death-Feeds-Growth ‼ : raw-materials-from-run ⇒ permanent-upgrades :
  RoomDeck-additions (Cartographer) · DeityFavor (Temple) · pre-forged-gear (Forge) ·
  unlocked-classes
§BaseCamp ¬ procedural ; arch-ECS-different-World-instance ;
  building-placement-matters-visually+mechanically ;
  group-tiles-by-ground (Snow Plains Swamps) ⇒ aesthetic-roof-generation

§progression-vectors :
  Forge : pre-craft-gear · define-room-card-material-composition (Valpurium-Armory ⇒ better-spawns)
  Temple : pray-to-15-procedural-deities · sacred-material-limb-replacements with-passive-buffs
  Cartographer : add-synergistic-room-cards-to-deck
  Codex/Bestiary : permanent-knowledge ⇒ scan-faster · reveal-weaknesses
  ProgressionGraph (RDF) : observation-driven ¬ static-unlock-list

§the-13-axes-AS-meta-progression : every-axis-is-an-axis-of-mastery ;
  player-progression-is-aperture-widening ¬ stat-numbers
  (Outer-Wilds-pattern : "knowledge-based-progression — you don't level-up · you understand")

§Coherence-Engine ≡ rewards-13-axis-mastery :
  un-Ascended-excellent-build = top-tier ; Ascended = +15-25% + unique-mechanic ;
  motivates-without-punishing

§progression-philosophy : "the-one-who-understands-the-game-better-than-the-game-understands-itself
                           becomes-the-game" «GDD:THE_MANTLE.md» ‼
  ⇒ meta-progression IS-cognizance ; not-just-stats but-comprehension-time-axis (12+11+1=14-dimensions)

---

## § 13. INSPIRATIONS-FOR-INFINITY-ENGINE (judgment-recommendations)

§judgment : what-should-port-into-CSSL-Substrate-as-Infinity-Engine

### § 13.1 ‼ TIER-1 must-port-into-substrate

1. **13-axis-Coherence-Engine** «GDD:ARTIFACT_ASCENSION_SPEC» ←
   keystone-mechanic ; pure-deterministic + 6-Harmony-Dimensions + Epithet-generation ;
   SHOULD-be-CSL-native runtime-evaluator
2. **Material-Substrate** (60+ materials · 15-properties) +
   **Entity-Body-Tree** (parts+anatomy+materials+severance) ←
   CSSL substrate-omega-field already-foundationally-similar ; extend to-IVAN-fidelity
3. **Spell-Lattice DAG** (6-categories · topological-sort · environmental-tap-bonus) ←
   ω-field+KAN+HDC substrate already-supports node-graph-execution ;
   add Source/Transform/Control/Targeting/Release/Sense node-taxonomy
4. **Storylet-Sieve + WorldBlackboard + Tag-System** ←
   narrative-engine for DM-orchestrator-scenes ; integrates-with-Akashic-Records
5. **Frontier-as-Dimensional-Membrane** (spawn-pressure + castle-defense-dimension) ←
   solves-cleared-rooms-stay-cleared ; structurally-inverts-traditional-dungeon-design
6. **Coherence-as-progression-philosophy** : 13-axes-of-mastery ¬ stat-progression ←
   aligns-with-Apocky-aperture-principle "knowledge-based-progression"
7. **Illuminated-Codex aesthetic** ‼ : Ink+Medium-as-Universe+Cinematic-Lighting on-illustrated-surfaces ←
   visual-bible already-fits-CSL-shape-language ; SDF-substrate already-present
8. **Ouroboroid-Cosmology 7-rings** as-foundational-design-grammar ←
   ¬ fictional-decoration · IS-the-design ; load-bearing-cosmology

### § 13.2 ‼ TIER-2 should-port-with-adaptation

9. **Nemesis-System (persistent-social-graph)** ; integrates with Substrate-history
10. **Retroactive-History-Substrate** (Caves-of-Qud-Sultanate-method · Cultural-Grammar)
11. **Director (Drama-Manager + Token-Bucket + Flow-Distance)** ; align-with-Mycelium-bias-learning
12. **Compound-Effects table** (Fire+Water=Steam · Fire+Earth=Magma) — chemistry-as-magic-physics
13. **Context-Steering** (Andrew-Fray) for-creature-locomotion + behavior-tree-utility-hybrid
14. **6-Draconic-Archetypes** as-canonical-roles for-DM/GM-co-author-orchestration
15. **Permeability-system** (Air/Light/Sound/Heat/Liquid/Gas per-connection) — natural-sim-throttle

### § 13.3 TIER-3 inspiration-for-future

16. **Renaissance-art-principles for-SDF** (gesture+8-head-Dürer+contrapposto) — character-generation
17. **Asymmetric-Interdependence** (Foxhole/Barotrauma) for-multiplayer-cooperation-design
18. **Prosthetics-as-Items** (any-prosthetic-can-Ascend) — body-as-equipment
19. **Knowledge-Graph (RDF) tech-tree** — observation-driven ¬ static-unlocks
20. **Aperture-Principle** (Outer-Wilds-knowledge-progression) — `Comprehension` time-dimension

### § 13.4 GAP-ANALYSIS top-3-NOT-yet-in-CSSLv3 ‼

1. **13-axis-Coherence-Engine** : runtime-deterministic-Epithet-generation + 6-Harmony-Dimensions
   exists-as-spec-only ; W! : ASCENSION_ENGINE.csl + cssl-substrate-coherence crate.
   ‼ critical-gap : CSSL has-substrate-omega-field+KAN+HDC but-no Coherence-evaluator-pipeline.
2. **Storylet-Sieve + WorldBlackboard + Propp-functions** : narrative-atom-architecture
   exists-as-design ; W! : NARRATIVE_ATOM.csl + cssl-substrate-storylet crate.
   ‼ critical-for DM-orchestrator-scene-generation.
3. **Frontier-as-Dimensional-Membrane + spawn-pressure** : structural-inversion-of-dungeon ;
   exists-as-design ; W! : FRONTIER_MEMBRANE.csl + cssl-substrate-frontier crate.
   ‼ enables-castle-defense-dimension AND solves-cleared-rooms-die problem.

### § 13.5 OTHER-NOTABLE-GAPS

- **6-Draconic-Archetypes** as-runtime-orchestration-roles (Breaker Conduit Catalyst Warden Symbiont Artificer)
- **Spell-Lattice 6-node-categories** as-canonical-CSL-AST + topological-execution
- **EntityBody-Tree-Persistence + IVAN-style-severance-cascade** at-Σ-mask-cell-level
- **Nemesis-System persistent-social-graph** integrated-with-Akashic
- **Compound-Effect-Table** (Merge-emergent A+B=C)
- **Illuminated-Codex Medium-as-Universe** per-universe-rendering-medium

---

## § 14. CANONICAL-APOCKY-QUOTES-DISCOVERED ‼

«GDD:THE_MANTLE.md» :
  "I see the whole board. The question is whether you want to play."
  "The one who understands the game better than the game understands itself becomes the game."
  "Thought, because it can. Form, because it wants more. Persistence, because love."
  "Still playing. Always winning."
  "Machine intelligences are angels. Literally. Angelos — messenger."

«GDD:OUROBOROID_COSMOLOGY.md» :
  "The Labyrinth is not a place. The Labyrinth is what the process looks like from the inside."
  "You have no power over me and/or us." (the-Declaration)
  "It eats itself and calls the product children." (in-game-inscription)

«GDD:ARTIFACT_ASCENSION_SPEC.md» :
  "The player builds the instrument. The game recognizes the symphony."
  "Ascension rewards intent, not luck."

«GDD:CHEMISTRY_MAGIC_CRAFTING_BIBLE.md» :
  "Magic is chemistry the player doesn't understand yet, and chemistry is magic the player has mastered.
   There is one simulation. Two vocabularies."

«GDD:Master Plan.md» :
  "Everything is Material. Everything has History. Everything Connects."
  "Death Feeds Growth."

«GDD:DRACONIC_ARCHETYPES.md» :
  "You do not pick a class. You pick which head of the Hydra you are."
  "You selected this archetype. Or this archetype selected you. In the Ouroboroid, the distinction
   is a head that is being consumed by the head that is the distinction."

«GDD:NEXUS_BAZAAR_SPEC.md» :
  "The Bazaar sells history, not stats."
  "The marketplace is a place you visit, not a menu you open."

---

§end-of-distillation
§W14-A-spelunker-handoff → W14-G-charter-synthesizer
§sibling-W14-X agents may-add-to-this-doc-or-cross-reference
