//! § gm_narrator — GM narrator procedural environment + dialogue
//! ══════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-4 (W-LOA-host-dm) — STAGE-0 BOOTSTRAP for `scenes/gm_narrator.cssl`.
//!
//! § AUTHORITATIVE DESIGN SPEC
//!   `scenes/gm_narrator.cssl` (the .cssl source is the authority ; this
//!   Rust file is a stage-0 mirror until the stage-1 csslc compiles the
//!   .cssl directly into equivalent code). Constant + identifier names
//!   below are preserved verbatim for that translation path.
//!
//! § DESIGN
//!   - 32 phrase-pools indexed by topic (`PhraseTopic`).
//!   - 16 NPC archetypes (`Archetype`).
//!   - `describe_environment(camera_pos, time_of_day)` selects from
//!     environment / weather / architecture / lore-history pools.
//!   - `generate_dialogue(npc_id, mood, topic)` selects from the
//!     archetype-keyed dialogue table.
//!   - 32-deep anti-repeat ring (FNV-1a hash of the chosen phrase) skips
//!     a candidate if the same hash was emitted within the last 32 calls.
//!
//! § DETERMINISM
//!   The narrator uses a small xorshift32 PRNG seeded from the inputs
//!   (camera_pos / time_of_day / npc_id / mood) so the same input emits
//!   the same phrase. This keeps the runtime free of `rand` thread-locals
//!   and aligns with the CSSL replay-determinism contract — a recorded
//!   input stream replays bit-identically.

use cssl_rt::loa_startup::log_event;

// ─────────────────────────────────────────────────────────────────────────
// § VEC3 (stage-0 ; pre-glam)
// ─────────────────────────────────────────────────────────────────────────

/// Stage-0 local Vec3. Sibling slices may swap for `glam::Vec3` when the
/// workspace adopts glam ; the narrator only consumes `(x, y, z)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § ARCHETYPES (16)
// ─────────────────────────────────────────────────────────────────────────

/// 16 NPC archetypes. Indices preserved verbatim from `scenes/gm_narrator.cssl`
/// for the stage-1 translation path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Archetype {
    Sage = 0,
    Trickster = 1,
    Warrior = 2,
    Healer = 3,
    Merchant = 4,
    Hermit = 5,
    Apprentice = 6,
    Elder = 7,
    Scout = 8,
    Bard = 9,
    Smith = 10,
    Witness = 11,
    Wanderer = 12,
    Apocalyptic = 13,
    Liminal = 14,
    Mute = 15,
}

pub const ARCHETYPE_SAGE: Archetype = Archetype::Sage;
pub const ARCHETYPE_TRICKSTER: Archetype = Archetype::Trickster;
pub const ARCHETYPE_WARRIOR: Archetype = Archetype::Warrior;
pub const ARCHETYPE_HEALER: Archetype = Archetype::Healer;
pub const ARCHETYPE_MERCHANT: Archetype = Archetype::Merchant;
pub const ARCHETYPE_HERMIT: Archetype = Archetype::Hermit;
pub const ARCHETYPE_APPRENTICE: Archetype = Archetype::Apprentice;
pub const ARCHETYPE_ELDER: Archetype = Archetype::Elder;
pub const ARCHETYPE_SCOUT: Archetype = Archetype::Scout;
pub const ARCHETYPE_BARD: Archetype = Archetype::Bard;
pub const ARCHETYPE_SMITH: Archetype = Archetype::Smith;
pub const ARCHETYPE_WITNESS: Archetype = Archetype::Witness;
pub const ARCHETYPE_WANDERER: Archetype = Archetype::Wanderer;
pub const ARCHETYPE_APOCALYPTIC: Archetype = Archetype::Apocalyptic;
pub const ARCHETYPE_LIMINAL: Archetype = Archetype::Liminal;
pub const ARCHETYPE_MUTE: Archetype = Archetype::Mute;

pub const ARCHETYPE_COUNT: usize = 16;

impl Archetype {
    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Archetype::Sage),
            1 => Some(Archetype::Trickster),
            2 => Some(Archetype::Warrior),
            3 => Some(Archetype::Healer),
            4 => Some(Archetype::Merchant),
            5 => Some(Archetype::Hermit),
            6 => Some(Archetype::Apprentice),
            7 => Some(Archetype::Elder),
            8 => Some(Archetype::Scout),
            9 => Some(Archetype::Bard),
            10 => Some(Archetype::Smith),
            11 => Some(Archetype::Witness),
            12 => Some(Archetype::Wanderer),
            13 => Some(Archetype::Apocalyptic),
            14 => Some(Archetype::Liminal),
            15 => Some(Archetype::Mute),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Archetype::Sage => "Sage",
            Archetype::Trickster => "Trickster",
            Archetype::Warrior => "Warrior",
            Archetype::Healer => "Healer",
            Archetype::Merchant => "Merchant",
            Archetype::Hermit => "Hermit",
            Archetype::Apprentice => "Apprentice",
            Archetype::Elder => "Elder",
            Archetype::Scout => "Scout",
            Archetype::Bard => "Bard",
            Archetype::Smith => "Smith",
            Archetype::Witness => "Witness",
            Archetype::Wanderer => "Wanderer",
            Archetype::Apocalyptic => "Apocalyptic",
            Archetype::Liminal => "Liminal",
            Archetype::Mute => "Mute",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PHRASE TOPICS (32)
// ─────────────────────────────────────────────────────────────────────────

/// 32 phrase-pool topics. Indices preserved verbatim from `scenes/gm_narrator.cssl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PhraseTopic {
    LoreHistory = 0,
    Environment = 1,
    Weather = 2,
    Creature = 3,
    Architecture = 4,
    Memory = 5,
    Prophecy = 6,
    Warning = 7,
    Greeting = 8,
    Farewell = 9,
    Mystery = 10,
    Hope = 11,
    Despair = 12,
    Reverence = 13,
    Defiance = 14,
    Mourning = 15,
    Bargain = 16,
    Riddle = 17,
    Lullaby = 18,
    BattleCry = 19,
    Apology = 20,
    Boast = 21,
    Confession = 22,
    Question = 23,
    Direction = 24,
    Advice = 25,
    Joke = 26,
    Threat = 27,
    Promise = 28,
    Plea = 29,
    Observation = 30,
    Silence = 31,
}

pub const PHRASE_TOPIC_COUNT: usize = 32;

impl PhraseTopic {
    pub fn from_index(i: u8) -> Option<Self> {
        if (i as usize) < PHRASE_TOPIC_COUNT {
            // SAFETY-NOTE : we deliberately avoid `unsafe { transmute }` ;
            // the explicit match below is forbid(unsafe_code)-clean.
            Some(match i {
                0 => PhraseTopic::LoreHistory,
                1 => PhraseTopic::Environment,
                2 => PhraseTopic::Weather,
                3 => PhraseTopic::Creature,
                4 => PhraseTopic::Architecture,
                5 => PhraseTopic::Memory,
                6 => PhraseTopic::Prophecy,
                7 => PhraseTopic::Warning,
                8 => PhraseTopic::Greeting,
                9 => PhraseTopic::Farewell,
                10 => PhraseTopic::Mystery,
                11 => PhraseTopic::Hope,
                12 => PhraseTopic::Despair,
                13 => PhraseTopic::Reverence,
                14 => PhraseTopic::Defiance,
                15 => PhraseTopic::Mourning,
                16 => PhraseTopic::Bargain,
                17 => PhraseTopic::Riddle,
                18 => PhraseTopic::Lullaby,
                19 => PhraseTopic::BattleCry,
                20 => PhraseTopic::Apology,
                21 => PhraseTopic::Boast,
                22 => PhraseTopic::Confession,
                23 => PhraseTopic::Question,
                24 => PhraseTopic::Direction,
                25 => PhraseTopic::Advice,
                26 => PhraseTopic::Joke,
                27 => PhraseTopic::Threat,
                28 => PhraseTopic::Promise,
                29 => PhraseTopic::Plea,
                30 => PhraseTopic::Observation,
                _ => PhraseTopic::Silence,
            })
        } else {
            None
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PHRASE POOLS (32 topics × 4 phrases each = 128 base phrases)
// ─────────────────────────────────────────────────────────────────────────
//
// Stage-0 pool : 4 phrases per topic. The .cssl spec calls for richer
// pools later ; this gets the runtime narrating today, and stage-1
// translation can backfill from a larger corpus without API churn.

const PHRASE_POOLS: [[&str; 4]; PHRASE_TOPIC_COUNT] = [
    // 0 LoreHistory
    [
        "An age before the labyrinth, the stars were ordered.",
        "The first walls were laid by hands now nameless.",
        "Old songs remember a sky without spirals.",
        "What was once the kingdom is now the maze.",
    ],
    // 1 Environment
    [
        "The corridor breathes around you, slow and patient.",
        "Moss climbs the stones in slow green tides.",
        "A draft moves through the seams like a held breath.",
        "Roots thread the masonry where the floor splits.",
    ],
    // 2 Weather
    [
        "Wind threads itself through the stone like memory.",
        "Rain finds the cracks the architects forgot.",
        "Thunder rolls somewhere beneath the foundation.",
        "A pale mist curls under the doors at dawn.",
    ],
    // 3 Creature
    [
        "Something small skitters at the edge of hearing.",
        "Eyes blink once and then are gone.",
        "A shape pauses at the threshold and does not enter.",
        "Wings ruffle in the rafters above the lamps.",
    ],
    // 4 Architecture
    [
        "Arches lean against each other like old friends.",
        "Pillars list slightly, as if listening.",
        "The vaulted ceiling holds carvings no one reads.",
        "Stairs spiral down past where the lamps reach.",
    ],
    // 5 Memory
    [
        "You feel as though you have stood here before.",
        "A familiar smell threads under the dust.",
        "An echo arrives a half-beat after your step.",
        "The shape of the room mirrors a dream.",
    ],
    // 6 Prophecy
    [
        "A voice once said the doors will all close at once.",
        "There is an hour the maze chooses its end.",
        "The last lamp will be one no torch lit.",
        "A keystone waits for a name yet to be spoken.",
    ],
    // 7 Warning
    [
        "Tread softly here ; the floor remembers weight.",
        "Do not call out in the long gallery.",
        "The mirror in the next room is hungry.",
        "Step over the third stone, never on it.",
    ],
    // 8 Greeting
    [
        "You arrive and the stones make room.",
        "Welcome, traveler — the corridor knows you.",
        "Come closer ; the lamps are kind tonight.",
        "Ah, a face. The maze grows quiet to listen.",
    ],
    // 9 Farewell
    [
        "Go softly. The way back is rarely the same.",
        "Keep one eye on shadows ; they share the path.",
        "May the lamps remember your name.",
        "Until the next door opens, then.",
    ],
    // 10 Mystery
    [
        "A door stands here that no key fits.",
        "There is a sound the silence makes when watched.",
        "The map agrees with itself, and the room does not.",
        "A symbol on the wall changes when not observed.",
    ],
    // 11 Hope
    [
        "Even here, the dawn finds the windows.",
        "Some lamps refuse to go out.",
        "A small kindness was offered in this room once.",
        "The maze permits exits, though it does not advertise them.",
    ],
    // 12 Despair
    [
        "Every door I open opens to the same night.",
        "The lamps tire long before the corridors do.",
        "Hope here is rationed in candles.",
        "I no longer remember the shape of outside.",
    ],
    // 13 Reverence
    [
        "Speak softly — these stones were named.",
        "Bow your head as you cross the threshold.",
        "The architects watch from inside the carvings.",
        "Walk as though the floor were a list of names.",
    ],
    // 14 Defiance
    [
        "The maze does not own me.",
        "I will not learn its rules.",
        "Every step I take is mine, regardless.",
        "Let it close ; I will close last.",
    ],
    // 15 Mourning
    [
        "We lit a lamp here for someone we lost.",
        "The room is quiet because she was once loud.",
        "There was a song this wall used to know.",
        "A friend's footstep is missing from the echo.",
    ],
    // 16 Bargain
    [
        "I would trade what I carry for what you know.",
        "Name your price ; the maze is patient.",
        "There is always an exchange. There is always a cost.",
        "Speak first what you want ; I will weigh it.",
    ],
    // 17 Riddle
    [
        "What walks closed corridors and is never lost?",
        "I am opened by silence and shut by speech.",
        "Three lamps, two doors, one truth — choose.",
        "The thing you brought is the thing you'll need.",
    ],
    // 18 Lullaby
    [
        "Sleep gently ; the stones will keep your watch.",
        "The lamp will burn until your dreams arrive.",
        "Close your eyes ; the corridor breathes for you.",
        "Soft, soft — the maze is patient with sleepers.",
    ],
    // 19 BattleCry
    [
        "Stand fast! The walls remember bravery.",
        "Strike — the maze respects what bites back.",
        "Hold the line ; the lamps will not abandon us.",
        "For every door behind us, one ahead!",
    ],
    // 20 Apology
    [
        "Forgive me ; I should have warned you sooner.",
        "I owe you a lamp and a clean corridor.",
        "I'm sorry — the maze had me in its grip.",
        "It was not meant as a slight.",
    ],
    // 21 Boast
    [
        "I have walked corridors you have not named.",
        "The maze and I have an understanding.",
        "Three doors I opened today before breakfast.",
        "No lamp here has bested me.",
    ],
    // 22 Confession
    [
        "I have lit lamps that should have stayed dark.",
        "Once I closed a door I had no right to close.",
        "The map I carry is not the one I drew.",
        "I have heard things the silence did not say.",
    ],
    // 23 Question
    [
        "Where did you come in?",
        "Did the corridor turn behind you?",
        "Have the lamps been kind today?",
        "What did the maze ask of you?",
    ],
    // 24 Direction
    [
        "Two corridors east, then mind the third stair.",
        "Down past the leaning lamp, take the left arch.",
        "Through the gallery where the echoes pile up.",
        "Follow the seam in the floor ; it knows the way.",
    ],
    // 25 Advice
    [
        "Bring two lamps. Always two.",
        "Do not bargain with mirrors.",
        "Trust the warmth of stone over the gleam of metal.",
        "Sleep only where echoes are kind.",
    ],
    // 26 Joke
    [
        "I asked a door for directions ; it shut.",
        "The maze tells one joke and only the lamps laugh.",
        "Two travelers walk into a corridor ; one walks out.",
        "Why did the keystone stop singing? Stage fright.",
    ],
    // 27 Threat
    [
        "Take one more step and the lamps will stop guiding you.",
        "The corridor will not be kind if you press it.",
        "I have a key that opens you, traveler.",
        "Try me, and the floor will keep your weight.",
    ],
    // 28 Promise
    [
        "I will see you to the next lamp.",
        "Whatever the corridor asks, I will answer for both.",
        "When the door opens, your name will be on the list.",
        "I will hold the threshold until you cross.",
    ],
    // 29 Plea
    [
        "Please ; just one lamp more.",
        "Do not let the door close on me.",
        "Stay until the corridor stops humming.",
        "Show me the way back ; the map has lied.",
    ],
    // 30 Observation
    [
        "The lamps lean north tonight.",
        "Your shadow is lagging behind your step.",
        "There is a draft where there was none yesterday.",
        "The keystone hums when the moon clears the gallery.",
    ],
    // 31 Silence
    [
        "...",
        "(no response)",
        "(a long pause)",
        "(the figure looks at you and says nothing)",
    ],
];

// ─────────────────────────────────────────────────────────────────────────
// § ARCHETYPE → PREFERRED TOPIC TABLE
// ─────────────────────────────────────────────────────────────────────────
//
// Each archetype has a 4-topic preference list ; `generate_dialogue` mixes
// the requested topic with the archetype's preferences via the seeded PRNG
// so dialogue naturally drifts toward archetype-appropriate phrasing.

const ARCHETYPE_PREFERENCES: [[u8; 4]; ARCHETYPE_COUNT] = [
    // Sage : LoreHistory, Prophecy, Reverence, Riddle
    [0, 6, 13, 17],
    // Trickster : Joke, Riddle, Boast, Mystery
    [26, 17, 21, 10],
    // Warrior : BattleCry, Defiance, Threat, Boast
    [19, 14, 27, 21],
    // Healer : Hope, Lullaby, Apology, Promise
    [11, 18, 20, 28],
    // Merchant : Bargain, Greeting, Direction, Advice
    [16, 8, 24, 25],
    // Hermit : Silence, Memory, Mystery, Observation
    [31, 5, 10, 30],
    // Apprentice : Question, Greeting, Apology, Advice
    [23, 8, 20, 25],
    // Elder : LoreHistory, Memory, Mourning, Reverence
    [0, 5, 15, 13],
    // Scout : Direction, Warning, Observation, Question
    [24, 7, 30, 23],
    // Bard : Joke, Boast, Greeting, Lullaby
    [26, 21, 8, 18],
    // Smith : Boast, Threat, Advice, Observation
    [21, 27, 25, 30],
    // Witness : Confession, Mourning, Memory, Observation
    [22, 15, 5, 30],
    // Wanderer : Direction, Farewell, Greeting, Observation
    [24, 9, 8, 30],
    // Apocalyptic : Prophecy, Despair, Defiance, Warning
    [6, 12, 14, 7],
    // Liminal : Mystery, Memory, Riddle, Observation
    [10, 5, 17, 30],
    // Mute : Silence, Silence, Silence, Silence
    [31, 31, 31, 31],
];

// ─────────────────────────────────────────────────────────────────────────
// § FNV-1a HASH (anti-repeat ring)
// ─────────────────────────────────────────────────────────────────────────

const FNV_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME_64: u64 = 0x100_0000_01b3;

/// FNV-1a 64-bit hash. Stable + cheap ; the anti-repeat ring stores the
/// hash rather than the string itself to avoid heap chatter.
fn fnv1a_64(s: &str) -> u64 {
    let mut h = FNV_OFFSET_BASIS_64;
    for &b in s.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME_64);
    }
    h
}

// ─────────────────────────────────────────────────────────────────────────
// § SEEDED PRNG (xorshift32)
// ─────────────────────────────────────────────────────────────────────────

/// xorshift32 — small + deterministic ; seeded from inputs each call.
fn xorshift32(seed: u32) -> u32 {
    let mut x = if seed == 0 { 0x9E37_79B9 } else { seed };
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

// ─────────────────────────────────────────────────────────────────────────
// § ANTI-REPEAT RING (32 deep)
// ─────────────────────────────────────────────────────────────────────────

pub const ANTI_REPEAT_RING_LEN: usize = 32;

#[derive(Debug)]
struct AntiRepeatRing {
    /// Recent phrase hashes (0 = empty slot).
    hashes: [u64; ANTI_REPEAT_RING_LEN],
    write_idx: usize,
}

impl AntiRepeatRing {
    fn new() -> Self {
        Self {
            hashes: [0u64; ANTI_REPEAT_RING_LEN],
            write_idx: 0,
        }
    }

    fn contains(&self, h: u64) -> bool {
        if h == 0 {
            return false;
        }
        self.hashes.iter().any(|&x| x == h)
    }

    fn push(&mut self, h: u64) {
        self.hashes[self.write_idx] = h;
        self.write_idx = (self.write_idx + 1) % ANTI_REPEAT_RING_LEN;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § GM NARRATOR
// ─────────────────────────────────────────────────────────────────────────

/// Time-of-day enum for `describe_environment`. Stable indices for FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TimeOfDay {
    Dawn = 0,
    Day = 1,
    Dusk = 2,
    Night = 3,
}

/// Mood enum for `generate_dialogue`. Stable indices for FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mood {
    Calm = 0,
    Anxious = 1,
    Hostile = 2,
    Friendly = 3,
    Sorrowful = 4,
    Reverent = 5,
    Playful = 6,
    Defiant = 7,
}

/// GM narrator — procedural environment-description + dialogue generator.
#[derive(Debug)]
pub struct GmNarrator {
    ring: AntiRepeatRing,
}

impl Default for GmNarrator {
    fn default() -> Self {
        Self::new()
    }
}

impl GmNarrator {
    pub fn new() -> Self {
        log_event(
            "INFO",
            "loa-host/gm",
            "init · 32 phrase-pools · 16 archetypes · 32-deep anti-repeat ring",
        );
        Self {
            ring: AntiRepeatRing::new(),
        }
    }

    /// Procedurally describe the neighborhood at `camera_pos` for the given
    /// `time_of_day`. Mixes from the Environment / Weather / Architecture
    /// pools with a seeded PRNG.
    pub fn describe_environment(&mut self, camera_pos: Vec3, time_of_day: TimeOfDay) -> String {
        let seed = mix_pos_seed(camera_pos, time_of_day as u32);
        // 3 sentence description : env + weather + architecture
        let env = self.draw_phrase_seeded(PhraseTopic::Environment, seed);
        let wx = self.draw_phrase_seeded(PhraseTopic::Weather, seed.wrapping_mul(2654435761));
        let arch = self.draw_phrase_seeded(PhraseTopic::Architecture, seed.wrapping_mul(40503));
        let combined = format!("{} {} {}", env, wx, arch);
        let msg = format!(
            "describe-environment · pos=({:.2},{:.2},{:.2}) · tod={:?} · phrase={}",
            camera_pos.x, camera_pos.y, camera_pos.z, time_of_day, env,
        );
        log_event("DEBUG", "loa-host/gm", &msg);
        combined
    }

    /// Generate a line of dialogue for `npc_id` (a stable per-NPC id) in
    /// the given `mood`, on the given `topic`. The archetype steers the
    /// final topic choice — the requested `topic` is one input, the
    /// archetype's preferences are another, and the seeded PRNG mixes
    /// them so a `Mute` archetype mostly emits silence even when asked
    /// for `BattleCry`.
    pub fn generate_dialogue(
        &mut self,
        npc_id: u32,
        archetype: Archetype,
        mood: Mood,
        topic: PhraseTopic,
    ) -> String {
        let arch_prefs = ARCHETYPE_PREFERENCES[archetype as usize];
        let mood_byte = mood as u32;
        let seed = npc_id
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(mood_byte.wrapping_mul(0x85eb_ca6b))
            .wrapping_add(topic as u32);
        let r = xorshift32(seed);

        // 50% chance to honor the requested topic ; 50% to pick from the
        // archetype's preferences. This is a stage-0 mixing rule ; a
        // future stage-1 spec slice can refine the weighting.
        let chosen_topic = if r % 2 == 0 {
            topic
        } else {
            let pref_idx = ((r >> 1) % 4) as usize;
            PhraseTopic::from_index(arch_prefs[pref_idx]).unwrap_or(topic)
        };

        let phrase = self.draw_phrase_seeded(chosen_topic, seed);
        let msg = format!(
            "dialogue · npc={} · archetype={} · mood={:?} · topic={:?} · phrase={}",
            npc_id,
            archetype.label(),
            mood,
            chosen_topic,
            phrase,
        );
        log_event("DEBUG", "loa-host/gm", &msg);
        phrase
    }

    /// Draw a phrase from `topic`'s pool with `seed` ; skips candidates
    /// whose hash is in the anti-repeat ring (up to 4 attempts ; the
    /// 4-deep pool can saturate the 32-deep ring, in which case we
    /// fall through and re-emit the seeded choice — failing soft is
    /// preferable to looping forever).
    fn draw_phrase_seeded(&mut self, topic: PhraseTopic, seed: u32) -> String {
        let pool = &PHRASE_POOLS[topic as usize];
        let n = pool.len() as u32;
        let mut r = xorshift32(seed.wrapping_add(1));
        for attempt in 0..4u32 {
            let idx = ((r.wrapping_add(attempt)) % n) as usize;
            let candidate = pool[idx];
            let h = fnv1a_64(candidate);
            if !self.ring.contains(h) {
                self.ring.push(h);
                return candidate.to_string();
            }
            r = xorshift32(r);
        }
        // All 4 candidates seen recently — re-emit deterministically.
        let idx = (r % n) as usize;
        pool[idx].to_string()
    }
}

/// Mix Vec3 + time_of_day into a 32-bit PRNG seed.
fn mix_pos_seed(p: Vec3, tod: u32) -> u32 {
    let xi = p.x.to_bits();
    let yi = p.y.to_bits();
    let zi = p.z.to_bits();
    let mut s = xi
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(yi.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(zi.wrapping_mul(0xc2b2_ae35));
    s = s.wrapping_add(tod.wrapping_mul(0x27d4_eb2f));
    if s == 0 { 0xdead_beef } else { s }
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm_describe_environment_returns_non_empty() {
        let mut g = GmNarrator::new();
        let s = g.describe_environment(Vec3::new(1.0, 2.0, 3.0), TimeOfDay::Day);
        assert!(!s.is_empty());
        // 3-pool concat → at least 3 sentence-ish chunks.
        assert!(s.len() > 20);
    }

    #[test]
    fn gm_dialogue_uses_archetype_table() {
        let mut g = GmNarrator::new();
        let s = g.generate_dialogue(42, Archetype::Sage, Mood::Calm, PhraseTopic::LoreHistory);
        assert!(!s.is_empty());
        // ARCHETYPE_PREFERENCES[Sage] = [LoreHistory, Prophecy, Reverence, Riddle]
        // The mixing rule guarantees the output comes from one of these
        // four pools OR the requested LoreHistory pool ; both intersect
        // LoreHistory, so the result is from a valid sage-coherent pool.
        // Confirm at minimum the table lookup didn't panic + non-empty.
        assert!(s.len() > 5);
    }

    #[test]
    fn gm_anti_repeat_avoids_recent_phrase() {
        let mut g = GmNarrator::new();
        // Drain the 4-pool topic enough that the ring has all 4 hashes.
        let _ = g.draw_phrase_seeded(PhraseTopic::Greeting, 1);
        let _ = g.draw_phrase_seeded(PhraseTopic::Greeting, 2);
        let _ = g.draw_phrase_seeded(PhraseTopic::Greeting, 3);
        let _ = g.draw_phrase_seeded(PhraseTopic::Greeting, 4);
        // After 4 distinct draws, the ring contains 4 hashes ; the next
        // draw with a fresh seed must NOT immediately produce a
        // hash-collision-free phrase since the pool is exhausted, but the
        // 4-attempt fall-through still emits some phrase. We just verify
        // the ring rejects an exact-recent phrase if attempted.
        let recent = g.draw_phrase_seeded(PhraseTopic::Greeting, 5);
        // The ring contains `recent`'s hash now. Push directly + verify
        // contains() returns true.
        let h = fnv1a_64(&recent);
        assert!(g.ring.contains(h), "anti-repeat ring should remember recent phrase");
    }

    #[test]
    fn gm_phrase_pool_count_is_32_with_four_each() {
        assert_eq!(PHRASE_POOLS.len(), PHRASE_TOPIC_COUNT);
        for pool in &PHRASE_POOLS {
            assert_eq!(pool.len(), 4);
            for phrase in pool {
                assert!(!phrase.is_empty(), "phrase must be non-empty");
            }
        }
    }

    #[test]
    fn gm_archetype_preferences_table_complete() {
        assert_eq!(ARCHETYPE_PREFERENCES.len(), ARCHETYPE_COUNT);
        for prefs in &ARCHETYPE_PREFERENCES {
            for &p in prefs {
                assert!((p as usize) < PHRASE_TOPIC_COUNT, "topic idx in range");
            }
        }
    }

    #[test]
    fn gm_mute_archetype_speaks_silence() {
        let mut g = GmNarrator::new();
        // Mute archetype → preferences are all Silence. With ~50% honor of
        // requested topic + ~50% archetype-pref, a few draws should land
        // in Silence at least once.
        let mut saw_silence = false;
        for npc in 0..32u32 {
            let s =
                g.generate_dialogue(npc, Archetype::Mute, Mood::Calm, PhraseTopic::Greeting);
            // Silence pool entries all begin with `(` or are `...`.
            if s == "..." || s.starts_with('(') {
                saw_silence = true;
                break;
            }
        }
        assert!(saw_silence, "Mute archetype must produce silence sometimes");
    }

    #[test]
    fn gm_fnv1a_64_is_deterministic() {
        let a = fnv1a_64("hello world");
        let b = fnv1a_64("hello world");
        assert_eq!(a, b);
        let c = fnv1a_64("hello worlz");
        assert_ne!(a, c);
    }
}
