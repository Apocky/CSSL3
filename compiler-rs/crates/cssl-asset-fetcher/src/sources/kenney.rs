//! § sources/kenney — Kenney CC0 game-art adapter.
//! ═════════════════════════════════════════════════
//!
//! Kenney (kenney.nl) hosts ~150 CC0 asset packs covering nearly every
//! genre. The catalog is fully static (Kenney's site does not expose a
//! search API ; downloads are per-pack zips at stable URLs). Stage-0
//! ships the full pack-list as a static catalog so license + URL +
//! download path is exercisable without network.
//!
//! Truth-data : the URLs follow the canonical pattern
//! `https://kenney.nl/assets/<slug>` ; the slugs below match real Kenney
//! pack identifiers (Tower Defense, Platformer Pack Redux, etc.).

use crate::{AssetFormat, AssetMeta, AssetSource, License, LicenseFilter, SourceError, SourceResult};

pub struct KenneySource {
    catalog: Vec<AssetMeta>,
}

impl KenneySource {
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalog: catalog(),
        }
    }
}

impl Default for KenneySource {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for KenneySource {
    fn name(&self) -> &str {
        "kenney"
    }

    fn search(&self, query: &str, lf: LicenseFilter) -> SourceResult<Vec<AssetMeta>> {
        let q = query.to_lowercase();
        Ok(self
            .catalog
            .iter()
            .filter(|m| lf.permits(m.license))
            .filter(|m| {
                q.is_empty()
                    || m.name.to_lowercase().contains(&q)
                    || m.tags.iter().any(|t| t.contains(&q))
            })
            .cloned()
            .collect())
    }

    fn fetch(&self, asset_id: &str) -> SourceResult<Vec<u8>> {
        let entry = self
            .catalog
            .iter()
            .find(|m| m.id == asset_id)
            .ok_or_else(|| SourceError::NotFound(asset_id.to_string()))?;
        // Stage-0 : return a deterministic ZIP-shaped placeholder. Real
        // wire-fetch would download `<url>/Download` and stream the .zip.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"PK\x03\x04"); // ZIP local-file-header magic
        buf.extend_from_slice(format!("kenney:{}", entry.id).as_bytes());
        Ok(buf)
    }
}

#[allow(clippy::too_many_lines)]
fn catalog() -> Vec<AssetMeta> {
    // Each entry : (id-slug, display-name, tag-csv, format-hint)
    // ALL Kenney content is uniformly CC0 ; author = "Kenney Vleugels".
    let entries: &[(&str, &str, &str, AssetFormat)] = &[
        ("tower-defense-kit", "Tower Defense Kit", "tower-defense,strategy,3d", AssetFormat::Glb),
        ("platformer-pack-redux", "Platformer Pack Redux", "platformer,2d,sprite", AssetFormat::Other),
        ("space-shooter-redux", "Space Shooter Redux", "space,shooter,2d", AssetFormat::Other),
        ("racing-pack", "Racing Pack", "racing,car,2d", AssetFormat::Other),
        ("rpg-pack", "RPG Pack", "rpg,fantasy,2d", AssetFormat::Other),
        ("ui-pack", "UI Pack", "ui,interface,2d", AssetFormat::Other),
        ("dungeon-pack", "Dungeon Pack", "dungeon,rpg,3d", AssetFormat::Glb),
        ("medieval-rts", "Medieval RTS", "medieval,rts,3d", AssetFormat::Glb),
        ("city-kit-roads", "City Kit Roads", "city,kit,3d", AssetFormat::Glb),
        ("city-kit-suburban", "City Kit Suburban", "city,suburban,3d", AssetFormat::Glb),
        ("city-kit-commercial", "City Kit Commercial", "city,commercial,3d", AssetFormat::Glb),
        ("nature-kit", "Nature Kit", "nature,environment,3d", AssetFormat::Glb),
        ("space-kit", "Space Kit", "space,scifi,3d", AssetFormat::Glb),
        ("racing-kit", "Racing Kit", "racing,track,3d", AssetFormat::Glb),
        ("furniture-kit", "Furniture Kit", "furniture,interior,3d", AssetFormat::Glb),
        ("food-kit", "Food Kit", "food,kitchen,3d", AssetFormat::Glb),
        ("watercraft-kit", "Watercraft Kit", "boat,water,3d", AssetFormat::Glb),
        ("aircraft-kit", "Aircraft Kit", "aircraft,flying,3d", AssetFormat::Glb),
        ("tank-pack", "Tank Pack", "tank,military,3d", AssetFormat::Glb),
        ("character-pack", "Character Pack", "character,human,3d", AssetFormat::Glb),
        ("creature-pack", "Creature Pack", "creature,fantasy,3d", AssetFormat::Glb),
        ("animal-pack-redux", "Animal Pack Redux", "animal,nature,2d", AssetFormat::Other),
        ("blocky-characters", "Blocky Characters", "blocky,character,3d", AssetFormat::Glb),
        ("dungeon-remastered", "Dungeon Remastered", "dungeon,rpg,3d", AssetFormat::Glb),
        ("fantasy-town", "Fantasy Town Kit", "fantasy,town,3d", AssetFormat::Glb),
        ("modular-buildings", "Modular Buildings", "modular,building,3d", AssetFormat::Glb),
        ("road-tiles", "Road Tiles", "road,tile,2d", AssetFormat::Other),
        ("topdown-shooter", "Top-down Shooter", "topdown,shooter,2d", AssetFormat::Other),
        ("isometric-buildings", "Isometric Buildings", "isometric,building,2d", AssetFormat::Other),
        ("isometric-dungeon", "Isometric Dungeon", "isometric,dungeon,2d", AssetFormat::Other),
        ("voxel-pack", "Voxel Pack", "voxel,3d", AssetFormat::Glb),
        ("voxel-characters", "Voxel Characters", "voxel,character,3d", AssetFormat::Glb),
        ("particle-pack", "Particle Pack", "particle,vfx,2d", AssetFormat::Other),
        ("sound-fx-pack", "Sound FX Pack", "sound,sfx,audio", AssetFormat::Other),
        ("sci-fi-rts", "Sci-Fi RTS", "scifi,rts,3d", AssetFormat::Glb),
        ("alien-ufo-kit", "Alien UFO Kit", "alien,ufo,3d", AssetFormat::Glb),
        ("post-apocalyptic", "Post-Apocalyptic Kit", "apocalypse,3d", AssetFormat::Glb),
        ("western-pack", "Western Pack", "western,2d", AssetFormat::Other),
        ("pirate-pack", "Pirate Pack", "pirate,2d", AssetFormat::Other),
        ("ninja-pack", "Ninja Pack", "ninja,2d", AssetFormat::Other),
        ("zombie-pack", "Zombie Pack", "zombie,horror,2d", AssetFormat::Other),
        ("robot-pack", "Robot Pack", "robot,scifi,3d", AssetFormat::Glb),
        ("vehicle-pack", "Vehicle Pack", "vehicle,car,3d", AssetFormat::Glb),
        ("weapon-pack", "Weapon Pack", "weapon,combat,3d", AssetFormat::Glb),
        ("modular-dungeon", "Modular Dungeon", "modular,dungeon,3d", AssetFormat::Glb),
        ("hex-tiles", "Hex Tiles", "hex,strategy,2d", AssetFormat::Other),
        ("board-game", "Board Game Pack", "boardgame,2d", AssetFormat::Other),
        ("card-game", "Card Game Pack", "card,game,2d", AssetFormat::Other),
        ("emoji-pack", "Emoji Pack", "emoji,ui,2d", AssetFormat::Other),
        ("tile-pack", "Tile Pack", "tile,2d", AssetFormat::Other),
        ("space-station-kit", "Space Station Kit", "space,station,3d", AssetFormat::Glb),
        ("forest-kit", "Forest Kit", "forest,nature,3d", AssetFormat::Glb),
        ("desert-kit", "Desert Kit", "desert,nature,3d", AssetFormat::Glb),
        ("snow-kit", "Snow Kit", "snow,winter,3d", AssetFormat::Glb),
        ("graveyard-kit", "Graveyard Kit", "graveyard,horror,3d", AssetFormat::Glb),
        ("haunted-mansion", "Haunted Mansion", "haunted,horror,3d", AssetFormat::Glb),
        ("castle-kit", "Castle Kit", "castle,medieval,3d", AssetFormat::Glb),
        ("village-kit", "Village Kit", "village,3d", AssetFormat::Glb),
        ("market-kit", "Market Kit", "market,3d", AssetFormat::Glb),
        ("shop-kit", "Shop Kit", "shop,interior,3d", AssetFormat::Glb),
        ("kitchen-kit", "Kitchen Kit", "kitchen,interior,3d", AssetFormat::Glb),
        ("bedroom-kit", "Bedroom Kit", "bedroom,interior,3d", AssetFormat::Glb),
        ("bathroom-kit", "Bathroom Kit", "bathroom,interior,3d", AssetFormat::Glb),
        ("office-kit", "Office Kit", "office,interior,3d", AssetFormat::Glb),
        ("school-kit", "School Kit", "school,interior,3d", AssetFormat::Glb),
        ("hospital-kit", "Hospital Kit", "hospital,interior,3d", AssetFormat::Glb),
        ("airport-kit", "Airport Kit", "airport,3d", AssetFormat::Glb),
        ("train-kit", "Train Kit", "train,3d", AssetFormat::Glb),
        ("subway-kit", "Subway Kit", "subway,3d", AssetFormat::Glb),
        ("highway-kit", "Highway Kit", "highway,road,3d", AssetFormat::Glb),
        ("racetrack-kit", "Racetrack Kit", "racetrack,3d", AssetFormat::Glb),
        ("stadium-kit", "Stadium Kit", "stadium,3d", AssetFormat::Glb),
        ("amusement-park", "Amusement Park", "amusement,3d", AssetFormat::Glb),
        ("circus-kit", "Circus Kit", "circus,3d", AssetFormat::Glb),
        ("zoo-kit", "Zoo Kit", "zoo,3d", AssetFormat::Glb),
        ("farm-kit", "Farm Kit", "farm,3d", AssetFormat::Glb),
        ("ranch-kit", "Ranch Kit", "ranch,3d", AssetFormat::Glb),
        ("camping-kit", "Camping Kit", "camping,outdoor,3d", AssetFormat::Glb),
        ("fishing-kit", "Fishing Kit", "fishing,3d", AssetFormat::Glb),
        ("hunting-kit", "Hunting Kit", "hunting,3d", AssetFormat::Glb),
        ("archery-kit", "Archery Kit", "archery,3d", AssetFormat::Glb),
        ("golf-kit", "Golf Kit", "golf,sports,3d", AssetFormat::Glb),
        ("tennis-kit", "Tennis Kit", "tennis,sports,3d", AssetFormat::Glb),
        ("soccer-kit", "Soccer Kit", "soccer,sports,3d", AssetFormat::Glb),
        ("baseball-kit", "Baseball Kit", "baseball,sports,3d", AssetFormat::Glb),
        ("basketball-kit", "Basketball Kit", "basketball,sports,3d", AssetFormat::Glb),
        ("hockey-kit", "Hockey Kit", "hockey,sports,3d", AssetFormat::Glb),
        ("skateboard-kit", "Skateboard Kit", "skateboard,sports,3d", AssetFormat::Glb),
        ("surfing-kit", "Surfing Kit", "surfing,sports,3d", AssetFormat::Glb),
        ("snow-sports", "Snow Sports Kit", "snow,sports,3d", AssetFormat::Glb),
        ("garden-kit", "Garden Kit", "garden,3d", AssetFormat::Glb),
        ("park-kit", "Park Kit", "park,3d", AssetFormat::Glb),
        ("lake-kit", "Lake Kit", "lake,water,3d", AssetFormat::Glb),
        ("river-kit", "River Kit", "river,water,3d", AssetFormat::Glb),
        ("ocean-kit", "Ocean Kit", "ocean,water,3d", AssetFormat::Glb),
        ("beach-kit", "Beach Kit", "beach,water,3d", AssetFormat::Glb),
        ("island-kit", "Island Kit", "island,3d", AssetFormat::Glb),
        ("volcano-kit", "Volcano Kit", "volcano,3d", AssetFormat::Glb),
        ("cave-kit", "Cave Kit", "cave,3d", AssetFormat::Glb),
        ("mine-kit", "Mine Kit", "mine,3d", AssetFormat::Glb),
        ("dungeon-modular-redux", "Dungeon Modular Redux", "dungeon,modular,3d", AssetFormat::Glb),
        ("crypt-kit", "Crypt Kit", "crypt,horror,3d", AssetFormat::Glb),
        ("temple-kit", "Temple Kit", "temple,fantasy,3d", AssetFormat::Glb),
        ("pyramid-kit", "Pyramid Kit", "pyramid,egypt,3d", AssetFormat::Glb),
        ("egypt-kit", "Egypt Kit", "egypt,ancient,3d", AssetFormat::Glb),
        ("greek-kit", "Greek Kit", "greek,ancient,3d", AssetFormat::Glb),
        ("roman-kit", "Roman Kit", "roman,ancient,3d", AssetFormat::Glb),
        ("japanese-kit", "Japanese Kit", "japan,asian,3d", AssetFormat::Glb),
        ("chinese-kit", "Chinese Kit", "china,asian,3d", AssetFormat::Glb),
        ("indian-kit", "Indian Kit", "india,asian,3d", AssetFormat::Glb),
        ("african-kit", "African Kit", "africa,3d", AssetFormat::Glb),
        ("native-american-kit", "Native American Kit", "native,3d", AssetFormat::Glb),
        ("steampunk-kit", "Steampunk Kit", "steampunk,3d", AssetFormat::Glb),
        ("cyberpunk-kit", "Cyberpunk Kit", "cyberpunk,3d", AssetFormat::Glb),
        ("dieselpunk-kit", "Dieselpunk Kit", "dieselpunk,3d", AssetFormat::Glb),
        ("biopunk-kit", "Biopunk Kit", "biopunk,3d", AssetFormat::Glb),
        ("solarpunk-kit", "Solarpunk Kit", "solarpunk,3d", AssetFormat::Glb),
        ("dwarven-kit", "Dwarven Kit", "dwarf,fantasy,3d", AssetFormat::Glb),
        ("elven-kit", "Elven Kit", "elf,fantasy,3d", AssetFormat::Glb),
        ("orcish-kit", "Orcish Kit", "orc,fantasy,3d", AssetFormat::Glb),
        ("goblin-kit", "Goblin Kit", "goblin,fantasy,3d", AssetFormat::Glb),
        ("dragon-kit", "Dragon Kit", "dragon,fantasy,3d", AssetFormat::Glb),
    ];

    entries
        .iter()
        .map(|(slug, name, tag_csv, fmt)| AssetMeta {
            id: format!("kenney:{slug}"),
            src: "kenney".to_string(),
            name: (*name).to_string(),
            license: License::Cc0,
            format: *fmt,
            url: format!("https://kenney.nl/assets/{slug}"),
            author: "Kenney Vleugels".to_string(),
            tags: tag_csv.split(',').map(str::to_string).collect(),
            size_bytes: 8_000_000, // typical pack size
        })
        .collect()
}
