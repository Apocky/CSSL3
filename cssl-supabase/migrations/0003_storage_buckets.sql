-- =====================================================================
-- § T11-WAVE3-SUPABASE · 0003_storage_buckets.sql
-- Storage buckets + per-bucket policies
--
-- Buckets
--   assets       : public-read · service-role-write  · max  50 MiB / file
--   screenshots  : own-read+write · public-read for is_public scene refs
--                                                    · max  10 MiB / file
--   audio        : own-read+write                    · max  10 MiB / file
--
-- Path conventions
--   assets/<source>/<source_id>.<ext>
--   screenshots/<user_id>/<scene_id>.<ext>
--   audio/<user_id>/<recording_id>.<ext>
-- =====================================================================

-- ---------------------------------------------------------------------
-- Bucket creation (idempotent)
-- ---------------------------------------------------------------------
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
    'assets', 'assets', true,
    52428800,  -- 50 MiB
    ARRAY[
        'model/gltf-binary',
        'model/gltf+json',
        'application/octet-stream',
        'model/obj',
        'application/vnd.ms-fbx',
        'image/png',
        'image/jpeg',
        'image/webp'
    ]
)
ON CONFLICT (id) DO UPDATE
    SET public = EXCLUDED.public,
        file_size_limit = EXCLUDED.file_size_limit,
        allowed_mime_types = EXCLUDED.allowed_mime_types;

INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
    'screenshots', 'screenshots', false,
    10485760,  -- 10 MiB
    ARRAY['image/png', 'image/jpeg', 'image/webp', 'image/avif']
)
ON CONFLICT (id) DO UPDATE
    SET public = EXCLUDED.public,
        file_size_limit = EXCLUDED.file_size_limit,
        allowed_mime_types = EXCLUDED.allowed_mime_types;

INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
    'audio', 'audio', false,
    10485760,  -- 10 MiB
    ARRAY['audio/wav', 'audio/mpeg', 'audio/ogg', 'audio/webm', 'audio/flac']
)
ON CONFLICT (id) DO UPDATE
    SET public = EXCLUDED.public,
        file_size_limit = EXCLUDED.file_size_limit,
        allowed_mime_types = EXCLUDED.allowed_mime_types;

-- ---------------------------------------------------------------------
-- assets bucket : public read · service-role write
-- ---------------------------------------------------------------------
DROP POLICY IF EXISTS "assets_storage_select_public"      ON storage.objects;
DROP POLICY IF EXISTS "assets_storage_insert_service"     ON storage.objects;
DROP POLICY IF EXISTS "assets_storage_update_service"     ON storage.objects;
DROP POLICY IF EXISTS "assets_storage_delete_service"     ON storage.objects;

CREATE POLICY "assets_storage_select_public"
    ON storage.objects FOR SELECT
    USING (bucket_id = 'assets');

CREATE POLICY "assets_storage_insert_service"
    ON storage.objects FOR INSERT
    WITH CHECK (bucket_id = 'assets' AND auth.role() = 'service_role');

CREATE POLICY "assets_storage_update_service"
    ON storage.objects FOR UPDATE
    USING (bucket_id = 'assets' AND auth.role() = 'service_role')
    WITH CHECK (bucket_id = 'assets' AND auth.role() = 'service_role');

CREATE POLICY "assets_storage_delete_service"
    ON storage.objects FOR DELETE
    USING (bucket_id = 'assets' AND auth.role() = 'service_role');

-- ---------------------------------------------------------------------
-- screenshots bucket
--   path = <user_id>/<scene_id>.<ext>
--   own-read+write · plus public-read when the referenced scene is is_public
-- ---------------------------------------------------------------------
DROP POLICY IF EXISTS "screenshots_storage_select_own_or_public" ON storage.objects;
DROP POLICY IF EXISTS "screenshots_storage_insert_own"           ON storage.objects;
DROP POLICY IF EXISTS "screenshots_storage_update_own"           ON storage.objects;
DROP POLICY IF EXISTS "screenshots_storage_delete_own"           ON storage.objects;

CREATE POLICY "screenshots_storage_select_own_or_public"
    ON storage.objects FOR SELECT
    USING (
        bucket_id = 'screenshots'
        AND (
            auth.role() = 'service_role'
            OR (storage.foldername(name))[1] = auth.uid()::text
            OR EXISTS (
                SELECT 1 FROM public.scenes s
                WHERE s.is_public = true
                  AND (storage.foldername(name))[1] = s.user_id::text
                  AND name LIKE '%/' || s.id::text || '.%'
            )
        )
    );

CREATE POLICY "screenshots_storage_insert_own"
    ON storage.objects FOR INSERT
    WITH CHECK (
        bucket_id = 'screenshots'
        AND (storage.foldername(name))[1] = auth.uid()::text
    );

CREATE POLICY "screenshots_storage_update_own"
    ON storage.objects FOR UPDATE
    USING (
        bucket_id = 'screenshots'
        AND (storage.foldername(name))[1] = auth.uid()::text
    )
    WITH CHECK (
        bucket_id = 'screenshots'
        AND (storage.foldername(name))[1] = auth.uid()::text
    );

CREATE POLICY "screenshots_storage_delete_own"
    ON storage.objects FOR DELETE
    USING (
        bucket_id = 'screenshots'
        AND (
            (storage.foldername(name))[1] = auth.uid()::text
            OR auth.role() = 'service_role'
        )
    );

-- ---------------------------------------------------------------------
-- audio bucket
--   path = <user_id>/<recording_id>.<ext>
--   own-read+write only · no public access
-- ---------------------------------------------------------------------
DROP POLICY IF EXISTS "audio_storage_select_own" ON storage.objects;
DROP POLICY IF EXISTS "audio_storage_insert_own" ON storage.objects;
DROP POLICY IF EXISTS "audio_storage_update_own" ON storage.objects;
DROP POLICY IF EXISTS "audio_storage_delete_own" ON storage.objects;

CREATE POLICY "audio_storage_select_own"
    ON storage.objects FOR SELECT
    USING (
        bucket_id = 'audio'
        AND (
            (storage.foldername(name))[1] = auth.uid()::text
            OR auth.role() = 'service_role'
        )
    );

CREATE POLICY "audio_storage_insert_own"
    ON storage.objects FOR INSERT
    WITH CHECK (
        bucket_id = 'audio'
        AND (storage.foldername(name))[1] = auth.uid()::text
    );

CREATE POLICY "audio_storage_update_own"
    ON storage.objects FOR UPDATE
    USING (
        bucket_id = 'audio'
        AND (storage.foldername(name))[1] = auth.uid()::text
    )
    WITH CHECK (
        bucket_id = 'audio'
        AND (storage.foldername(name))[1] = auth.uid()::text
    );

CREATE POLICY "audio_storage_delete_own"
    ON storage.objects FOR DELETE
    USING (
        bucket_id = 'audio'
        AND (
            (storage.foldername(name))[1] = auth.uid()::text
            OR auth.role() = 'service_role'
        )
    );
