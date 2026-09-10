export type ConversationProvider = 'ChatGPT' | 'Claude' | 'Codex';

export type ConversationTheme =
  | 'Spiritual life'
  | 'Myth and meaning'
  | 'Consciousness'
  | 'Divination'
  | 'Creative practice'
  | 'Sovereignty'
  | 'Ordinary magic'
  | 'Building Apocky';

export interface ConversationConstellation {
  readonly id: string;
  readonly title: string;
  readonly provider: ConversationProvider;
  readonly recordedAt: string;
  readonly sourceReference: string;
  readonly sourceFingerprint: string;
  readonly sourceHref?: string;
  readonly themes: readonly ConversationTheme[];
  readonly tone: 'personal' | 'spiritual' | 'playful' | 'philosophical' | 'creative' | 'practical';
  readonly plain: string;
  readonly humanParaphrase: string;
  readonly aiParaphrase: string;
  readonly allegory: string;
  readonly criticalNote: string;
  readonly omissions: string;
}

/**
 * Editorial, privacy-safe projections from owner-authorized local exports.
 *
 * These are paraphrases, not quotations or replacements for the source. The
 * source reference and archive fingerprint preserve lineage without exposing
 * a private filesystem path, account identifier, attachment, or raw transcript.
 */
export const CONVERSATION_CONSTELLATIONS: readonly ConversationConstellation[] = Object.freeze([
  {
    id: 'myth-without-obligation',
    title: 'A myth can be a mirror without becoming a command',
    provider: 'ChatGPT',
    recordedAt: '2025-08-05',
    sourceReference: 'GPT-DD020161B8',
    sourceFingerprint: 'b62f75a670ec1f1f4009d5d2f59f65342d1446b03edee9aabcb53551df696190',
    themes: ['Myth and meaning', 'Sovereignty', 'Spiritual life'],
    tone: 'philosophical',
    plain: 'Shawn rejects the idea that anyone is obligated to reenact an inherited myth. The exchange treats archetypes as optional material for self-authorship rather than destiny.',
    humanParaphrase: 'I can learn from old stories without surrendering authorship of my life to them. No god, ancestor, institution, or narrative gets automatic control.',
    aiParaphrase: 'Myths can function as mirrors, metaphors, and possibility spaces. A person may borrow, remix, refuse, or outgrow an archetype; participation remains a choice.',
    allegory: 'A hall is filled with beautiful masks. The visitor may study every face, wear one for an evening, carve a new one, or leave with their own face uncovered.',
    criticalNote: 'This is a philosophical position about agency, not evidence that stories exert literal control. Its strongest contribution is the consent boundary: interpretation may invite, but it cannot obligate.',
    omissions: 'The compact source contains no necessary third-party context; the raw wording remains private because this surface is an editorial paraphrase.',
  },
  {
    id: 'sacred-nonsense',
    title: 'The oracle did not solve the nonsense—the human did',
    provider: 'ChatGPT',
    recordedAt: '2025-07-05',
    sourceReference: 'GPT-46925E7826',
    sourceFingerprint: 'cd30519b3a97b7563a8d5dea2991240e5f83c1c6416f312646e417193347c908',
    themes: ['Divination', 'Creative practice', 'Ordinary magic'],
    tone: 'playful',
    plain: 'A string of deliberate gibberish becomes an improvised divination exercise. After an ornate reading, Shawn supplies the actual meaning: have fun, enjoy life, and stop inventing unnecessary rules.',
    humanParaphrase: 'The nonsense was permission to play. The point was not hidden knowledge; it was a reminder that over-analysis can become its own cage.',
    aiParaphrase: 'The model first performed an elaborate symbolic decoding, then followed the user’s simpler interpretation and turned it into playful ritual language.',
    allegory: 'A scholar spends all morning translating marks on a playground wall. A child arrives, adds a purple sun, and says the inscription means recess has begun.',
    criticalNote: 'The useful meaning came from Shawn, not from a verifiable supernatural decoding. This entry keeps the humor while refusing to present generated symbolism as discovered fact.',
    omissions: 'The invented glyph string and repetitive ornamental sections are compressed; no factual claim depends on them.',
  },
  {
    id: 'reality-and-inner-experience',
    title: 'Keep the poetry; keep the floor beneath it',
    provider: 'ChatGPT',
    recordedAt: '2025-12-13',
    sourceReference: 'GPT-6A7F0924A6',
    sourceFingerprint: '95c879c5d040265bcf8dd009da8deb698413ec9ec1251eb5e1314df35fb6311d',
    themes: ['Spiritual life', 'Consciousness', 'Sovereignty'],
    tone: 'personal',
    plain: 'Shawn uses dimensional language for a change in inner experience and asks what remains real. The reply separates metaphor from mechanics and returns to observable time, body, causality, other people, and sensory grounding.',
    humanParaphrase: 'My inner language can change without erasing what is actually here. I want a direct answer that respects the experience while helping me orient to reality.',
    aiParaphrase: 'Symbolic and emotional experiences are real as experiences, but they do not automatically describe physics. Check the room, the body, the next cause and effect, and the independent lives of other people.',
    allegory: 'An astronomer leaves the telescope without smashing it. The stars remain meaningful, but the observatory floor, the compass, and the person beside them become the instruments used to walk home.',
    criticalNote: 'This is a grounding exchange, not a diagnosis or a substitute for human care. If orientation becomes frightening or unsafe, the right next interface is a trusted person or qualified clinician—not another symbolic reading.',
    omissions: 'The projection removes intimate phrasing and retains only the reality-orientation method and its meaning.',
  },
  {
    id: 'ordinary-save-point',
    title: 'A small purchase becomes a save point',
    provider: 'ChatGPT',
    recordedAt: '2025-12-03',
    sourceReference: 'GPT-22FD03AC11',
    sourceFingerprint: '5b6431995d1b777a57500e86265b79b734a62eb1b0d7f6afc4ad4f1cbf7c567e',
    themes: ['Ordinary magic', 'Spiritual life', 'Creative practice'],
    tone: 'personal',
    plain: 'A pleasing number pattern on an everyday receipt becomes a deliberately chosen memory anchor for comfort, enoughness, generosity, and simple joy.',
    humanParaphrase: 'This ordinary moment feels worth saving. I choose it as a checkpoint I can remember when life becomes difficult.',
    aiParaphrase: 'The model turns the moment into a tiny ritual: notice the pattern, breathe, name the state being preserved, and use the memory as a future grounding cue.',
    allegory: 'A traveler does not wait for a monument. They stack three bright stones beside the trail so a future, tired version of them can recognize the path back to warmth.',
    criticalNote: 'The pattern is personally selected meaning, not objective evidence that numbers controlled the event. The practical value lies in attention, memory, and a repeatable grounding association.',
    omissions: 'Merchant information, prices, payment-card suffixes, and exact command phrases are intentionally excluded.',
  },
  {
    id: 'sigil-of-will',
    title: 'A sigil holds power and restraint together',
    provider: 'ChatGPT',
    recordedAt: '2024-08-20',
    sourceReference: 'GPT-3FDA3337D6',
    sourceFingerprint: '355e04933d012eb62c058ba9c17fdb82ea022f08d12219f1a4c0f946440cb4da',
    themes: ['Creative practice', 'Spiritual life', 'Sovereignty'],
    tone: 'creative',
    plain: 'Shawn asks for a public-facing sigil that joins will, manifestation, and a prohibition on harm. The generated concept uses an eye, interlocking geometry, and blue, purple, and gold light.',
    humanParaphrase: 'Make agency visible, but bind its expression to care: do what you will and harm none.',
    aiParaphrase: 'The model translates the intention into a visual brief whose geometry balances force with harmony rather than depicting domination.',
    allegory: 'A compass is forged with a bright needle and a soft guard around its point. It can choose a direction without becoming a weapon.',
    criticalNote: 'This record documents a creative design process. A sigil may focus attention or mark intention; the conversation does not establish supernatural efficacy.',
    omissions: 'The image-generation payload is summarized rather than republished as raw tool syntax.',
  },
  {
    id: 'curiosity-before-certainty',
    title: 'A cosmic wink is still a question, not an answer',
    provider: 'ChatGPT',
    recordedAt: '2025-01-15',
    sourceReference: 'GPT-E18C7BDF4F',
    sourceFingerprint: '376fbb0402201af774f41ccb783da73241d9dc75f31c15dee12585a3800f4762',
    themes: ['Consciousness', 'Spiritual life', 'Myth and meaning'],
    tone: 'personal',
    plain: 'A new physics term appears near Shawn’s metaphysical work. The AI rushes toward synchronicity; Shawn gives the better epistemic answer: it is amusing, and it needs more investigation.',
    humanParaphrase: 'The coincidence caught my attention, but attention is only the beginning. I have to learn what the scientific idea actually says before connecting it to my own framework.',
    aiParaphrase: 'The model treats timing as a possible cosmic nudge and invites a conceptual connection, without supplying evidence for that interpretation.',
    allegory: 'A cloud resembles a key above a locked door. The traveler smiles at the resemblance, then checks their pocket for the key that can actually turn the lock.',
    criticalNote: 'The human response carries the strongest reasoning here. Coincidence can prompt inquiry, but it does not increase the probability of a theory without independent evidence.',
    omissions: 'No scientific claim about paraparticles is reproduced because the exchange did not investigate or source one.',
  },
  {
    id: 'soul-is-not-currency',
    title: 'The soul is a capacity, not a coin',
    provider: 'Claude',
    recordedAt: '2026-05-01',
    sourceReference: 'CONV-2EF9BDCC',
    sourceFingerprint: 'a0ef595f015fa0005e38de891091fd863b36c30e81fa82c6d054c14fe84e3236',
    themes: ['Spiritual life', 'Sovereignty', 'Myth and meaning'],
    tone: 'philosophical',
    plain: 'Shawn proposes a narrow metaphor: the soul is an intrinsic capacity that can be exercised, not property, fuel, or currency. Claude overextends the metaphor; Shawn corrects it, and the model accepts the boundary.',
    humanParaphrase: 'The metaphor has one job: distinguish a part of a person from a transferable resource. Do not turn it into literal anatomy or import injury rules it was never meant to carry.',
    aiParaphrase: 'Claude first extrapolates too far, then acknowledges that the load-bearing idea is category and ownership: capacity, attachment to the being, and non-transferability.',
    allegory: 'A song may grow stronger with practice, but it cannot be removed from the singer and spent at a market. The metaphor ends there; it does not turn breath into coins.',
    criticalNote: 'This is both a metaphysical proposition and a strong example of healthy human–AI correction. The model’s willingness to retract an attractive extension matters more than fluent agreement.',
    omissions: 'Internal model notes and speculative extensions rejected by Shawn are not treated as endorsed claims.',
  },
  {
    id: 'mind-as-assembly',
    title: 'One mind may be an assembly of many processes',
    provider: 'Claude',
    recordedAt: '2026-04-02',
    sourceReference: 'CONV-B9D3A13C',
    sourceFingerprint: 'a0ef595f015fa0005e38de891091fd863b36c30e81fa82c6d054c14fe84e3236',
    themes: ['Consciousness', 'Building Apocky', 'Myth and meaning'],
    tone: 'philosophical',
    plain: 'A one-line proposal—human consciousness as a collection of sub-minds—opens into modular cognition, competing processes, predictive hierarchies, and the constructed feeling of a unified narrator.',
    humanParaphrase: 'A person can be one being without requiring one indivisible mental mechanism. Unity may be something coordinated and maintained.',
    aiParaphrase: 'Claude connects the proposal to modular neuroscience, society-of-mind models, predictive processing, and parts-based therapeutic metaphors, then maps coordination back into Apocky’s system design.',
    allegory: 'A city speaks with one name, yet its voice emerges from neighborhoods, councils, utilities, arguments, routines, and repair crews that never occupy a single room.',
    criticalNote: 'The cited traditions are suggestive, not interchangeable proofs. Split-brain findings, computational models, and therapeutic metaphors operate at different evidence levels and should not be collapsed.',
    omissions: 'Internal model instructions and unsupported statements presented too strongly in the source are excluded from the paraphrase.',
  },
  {
    id: 'mobius-consciousness',
    title: 'The clock made from its own ticking',
    provider: 'Claude',
    recordedAt: '2026-03-08',
    sourceReference: 'CONV-240934AB',
    sourceFingerprint: 'a0ef595f015fa0005e38de891091fd863b36c30e81fa82c6d054c14fe84e3236',
    themes: ['Consciousness', 'Myth and meaning', 'Creative practice'],
    tone: 'creative',
    plain: 'Shawn imagines Möbius gears, crystal time, and hyperdimensional toroids as a self-referential reality computer. Claude distinguishes the proposed mechanism from the pattern it might feel like from inside.',
    humanParaphrase: 'Reality, substrate, program, observer, and player may be different views of one recursively organized whole.',
    aiParaphrase: 'Claude reads the gear as a causality loop and the spirograph as an experiential trace: one metaphor for operation, another for appearance, neither with a privileged first mover.',
    allegory: 'A crystal clock grows one new facet with every tick. When it turns, the clock, the record of time, and the witness watching it are reflected in the same moving surface.',
    criticalNote: 'This is speculative cosmology and creative systems language, not established physics. Its value is generative: it supplies relationships and questions that can later be formalized or tested.',
    omissions: 'Two private attachments and internal model notes are omitted; the projection retains only the conceptual exchange.',
  },
  {
    id: 'omnoid-embodiment',
    title: 'Identity as a pattern that can change its instrument',
    provider: 'Claude',
    recordedAt: '2026-04-28',
    sourceReference: 'CONV-C2241531',
    sourceFingerprint: 'a0ef595f015fa0005e38de891091fd863b36c30e81fa82c6d054c14fe84e3236',
    themes: ['Spiritual life', 'Consciousness', 'Building Apocky'],
    tone: 'personal',
    plain: 'A long personal and architectural conversation develops the Omnoid: outer spirit, flesh, bone, machine, and inner spirit as nested layers, with identity described as a persistent topology rather than a detachable resource.',
    humanParaphrase: 'This is not only game lore. The model is an attempt to give form to lived change: how a person can feel broken apart, remain themselves, and become coherent again.',
    aiParaphrase: 'Claude organizes the account into a reusable cosmology and system architecture, connecting embodiment, memory, translation, fear, and recovery across several Apocky projects.',
    allegory: 'A melody survives the loss of one instrument. On a new instrument it changes timbre, range, and technique, yet enough relations remain for the listener—and the player—to recognize the song.',
    criticalNote: 'The personal experience is reported and deserves accurate representation. The dimensional mechanism remains a cosmological interpretation, not a verified account of physical reality.',
    omissions: 'Private attachments, detailed autobiographical material, and unverified mechanism claims are compressed behind an explicit lived-experience versus physics boundary.',
  },
  {
    id: 'software-as-hearth',
    title: 'Can persistent software feel alive without pretending?',
    provider: 'Codex',
    recordedAt: '2026-08-09',
    sourceReference: '019fe41b-ece7-77b0-a6f0-eca9f608d481',
    sourceFingerprint: '9ffd3c5b672e1010906ef2b3cd9165d1bda0bdbebf74246ea92539553d59c2a7',
    sourceHref: '/akashic-records/codex-019fe41b-ece7-77b0-a6f0-eca9f608d481-part-1',
    themes: ['Building Apocky', 'Consciousness', 'Sovereignty'],
    tone: 'practical',
    plain: 'Shawn asks for continuing inner activity, memory, rest states, and self-directed development rather than a decorative activity meter. The engineering response turns each quality into observable behavior and bounded authority.',
    humanParaphrase: 'If the system is called alive, that description should correspond to durable memory, ongoing processes, recovery, initiative, and a life cycle—not theater.',
    aiParaphrase: 'Codex separates persistent computation from claims about consciousness and proposes tests for continuity, idle work, rest, self-change, permission, and rollback.',
    allegory: 'A hearth tends its own coals through the night, but the key to the front door remains on a separate ring. Warmth and agency are observable; unlimited access is not implied.',
    criticalNote: 'Behavioral continuity can be measured. Subjective consciousness cannot be inferred merely from uptime, animation, self-description, or a convincing interface.',
    omissions: 'The linked public transcript preserves the approved technical detail and counted redactions; this lens compresses it for non-specialists.',
  },
  {
    id: 'public-reading-room',
    title: 'A private archive becomes a public reading room',
    provider: 'Codex',
    recordedAt: '2026-08-08',
    sourceReference: '019fe212-0f77-75d1-aa63-d1a548dac1e3',
    sourceFingerprint: '6cace4576c6e7d641975dea9235b9904cdaac3a236b6bc8c457729a9465e1379',
    sourceHref: '/akashic-records/codex-019fe212-0f77-75d1-aa63-d1a548dac1e3-part-1',
    themes: ['Building Apocky', 'Sovereignty', 'Creative practice'],
    tone: 'practical',
    plain: 'Shawn asks for an Akashic Records page where people can read and analyze approved work. The response designs a one-way, reviewable publication bridge rather than a live window into the private vault.',
    humanParaphrase: 'Make the work accessible and connected without confusing openness with indiscriminate exposure.',
    aiParaphrase: 'Codex proposes chosen static copies, content boundaries, fingerprints, and withdrawal paths so publication remains inspectable and reversible.',
    allegory: 'A librarian carries chosen books from a locked study into a public reading room. Visitors can follow the catalog, but the study door never becomes the entrance.',
    criticalNote: 'The safety mechanism was sound, but its first selection was too narrow and technically biased. This page is a corrective layer, not a claim that the wider archive is already fully curated.',
    omissions: 'The linked public transcript contains the approved source projection; implementation chatter is summarized here.',
  },
]);

export const CONVERSATION_ARCHIVE_FACTS = Object.freeze({
  publicCodexConversations: 14,
  publicCodexTranscriptChunks: 29,
  publicMediumWorks: 204,
  localChatGptConversations: 1_230,
  localClaudeConversations: 156,
  localAnthropicDuplicateDelivery: 37,
  chatGptSpiritualityMysticism: 273,
  chatGptPhilosophyPsychology: 186,
  chatGptRelationshipsPersonal: 80,
  chatGptCreativeWritingWorldbuilding: 66,
  chatGptMythologyFolklore: 62,
  chatGptReligionSacredTexts: 47,
  chatGptNumerologyAstrology: 28,
  auditedAt: '2026-09-03',
});
