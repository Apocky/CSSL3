-- =====================================================================
-- § T11-WAVE3-SUPABASE · seed.sql
-- 10 example asset rows · public-domain or permissive licensing
-- All attributions preserved per upstream license terms.
-- Run as service-role (RLS would otherwise block anon INSERT).
-- =====================================================================

INSERT INTO public.assets (
    source, source_id, name, license, attribution, format,
    upstream_url, metadata, bytes, indexed_at
) VALUES
-- 1. Stanford Bunny -- canonical research mesh
(
    'stanford-3d-scan', 'bunny',
    'Stanford Bunny',
    'public-domain',
    'Stanford 3D Scanning Repository (Greg Turk and Marc Levoy, 1994)',
    'ply',
    'https://graphics.stanford.edu/data/3Dscanrep/bunny.tar.gz',
    jsonb_build_object(
        'tags', ARRAY['benchmark', 'research', 'mesh'],
        'polycount', 69451,
        'thumbnail_url', 'https://graphics.stanford.edu/data/3Dscanrep/bunny.gif'
    ),
    20140000,
    now()
),
-- 2. Stanford Dragon
(
    'stanford-3d-scan', 'dragon',
    'Stanford Dragon',
    'public-domain',
    'Stanford 3D Scanning Repository (Stanford CG Lab, 1996)',
    'ply',
    'https://graphics.stanford.edu/data/3Dscanrep/dragon_recon/dragon_vrip.ply.gz',
    jsonb_build_object(
        'tags', ARRAY['benchmark', 'research', 'mesh'],
        'polycount', 871414
    ),
    35400000,
    now()
),
-- 3. Utah Teapot
(
    'public-domain', 'utah-teapot',
    'Utah Teapot',
    'public-domain',
    'Martin Newell, University of Utah, 1975',
    'obj',
    'https://www.cs.utah.edu/~natevm/newell_teaset/teapot.obj',
    jsonb_build_object(
        'tags', ARRAY['icon', 'benchmark'],
        'polycount', 6320
    ),
    312000,
    now()
),
-- 4. NASA Curiosity Rover -- 3D Resources NASA
(
    'nasa-3d', 'curiosity',
    'Mars Curiosity Rover',
    'public-domain',
    'NASA / JPL-Caltech (NASA 3D Resources)',
    'glb',
    'https://nasa3d.arc.nasa.gov/shared_assets/models/msl-curiosity/curiosity.glb',
    jsonb_build_object(
        'tags', ARRAY['nasa', 'rover', 'mars'],
        'thumbnail_url', 'https://nasa3d.arc.nasa.gov/images/msl-curiosity-thumb.jpg'
    ),
    18500000,
    now()
),
-- 5. NASA Apollo 11 LM
(
    'nasa-3d', 'apollo11-lm',
    'Apollo 11 Lunar Module Eagle',
    'public-domain',
    'NASA (NASA 3D Resources)',
    'glb',
    'https://nasa3d.arc.nasa.gov/shared_assets/models/apollo11-lm/lm.glb',
    jsonb_build_object(
        'tags', ARRAY['nasa', 'apollo', 'historical']
    ),
    9800000,
    now()
),
-- 6. Kenney Mini Dungeon -- knight
(
    'kenney', 'mini-dungeon-knight',
    'Mini Dungeon Knight',
    'CC0',
    'Kenney.nl (kenney.nl/assets/mini-dungeon)',
    'glb',
    'https://kenney.nl/media/pages/assets/mini-dungeon/knight.glb',
    jsonb_build_object(
        'tags', ARRAY['kenney', 'character', 'low-poly', 'fantasy'],
        'polycount', 480
    ),
    142000,
    now()
),
-- 7. Kenney Mini Dungeon -- skeleton
(
    'kenney', 'mini-dungeon-skeleton',
    'Mini Dungeon Skeleton',
    'CC0',
    'Kenney.nl (kenney.nl/assets/mini-dungeon)',
    'glb',
    'https://kenney.nl/media/pages/assets/mini-dungeon/skeleton.glb',
    jsonb_build_object(
        'tags', ARRAY['kenney', 'character', 'low-poly', 'fantasy', 'undead'],
        'polycount', 412
    ),
    128000,
    now()
),
-- 8. Kenney Nature Pack -- tree
(
    'kenney', 'nature-pack-tree-oak',
    'Nature Pack Oak Tree',
    'CC0',
    'Kenney.nl (kenney.nl/assets/nature-kit)',
    'glb',
    'https://kenney.nl/media/pages/assets/nature-kit/tree-oak.glb',
    jsonb_build_object(
        'tags', ARRAY['kenney', 'environment', 'low-poly', 'tree']
    ),
    96000,
    now()
),
-- 9. Polyhaven HDRI -- preview metadata only (real binary fetched on demand)
(
    'polyhaven', 'kloppenheim_06_puresky',
    'Kloppenheim 06 Pure Sky',
    'CC0',
    'Polyhaven.com (Greg Zaal, Sergej Majboroda)',
    'glb',
    'https://polyhaven.com/a/kloppenheim_06_puresky',
    jsonb_build_object(
        'tags', ARRAY['polyhaven', 'hdri', 'environment', 'sky'],
        'resolution', '8k',
        'thumbnail_url', 'https://cdn.polyhaven.com/asset_img/thumbs/kloppenheim_06_puresky.png'
    ),
    25400000,
    now()
),
-- 10. Khronos sample -- DamagedHelmet (canonical PBR test)
(
    'khronos-glTF-samples', 'DamagedHelmet',
    'Damaged Helmet (PBR reference)',
    'CC-BY-4.0',
    'theblueturtle_ (Khronos glTF Sample Models repo)',
    'glb',
    'https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Models/master/2.0/DamagedHelmet/glTF-Binary/DamagedHelmet.glb',
    jsonb_build_object(
        'tags', ARRAY['khronos', 'pbr', 'benchmark', 'reference'],
        'polycount', 15452
    ),
    3712000,
    now()
)
ON CONFLICT (source, source_id) DO NOTHING;
