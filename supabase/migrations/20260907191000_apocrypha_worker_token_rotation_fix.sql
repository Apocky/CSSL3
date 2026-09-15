-- Fix output-column ambiguity in the production worker-token rotation RPC.

CREATE OR REPLACE FUNCTION public.apocrypha_rotate_worker_token(p_node_id uuid)
RETURNS TABLE (node_id uuid, node_token text, token_version integer)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node  public.apocrypha_worker_node;
    v_token text;
BEGIN
    v_token := 'apn_' || encode(gen_random_bytes(32), 'hex');

    UPDATE public.apocrypha_worker_node AS worker_node
    SET token_hash = public.apocrypha_sha256(v_token),
        token_last_four = right(v_token, 4),
        token_version = worker_node.token_version + 1,
        token_issued_at = now()
    WHERE worker_node.id = p_node_id
      AND worker_node.status <> 'revoked'
    RETURNING worker_node.* INTO v_node;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'active or draining worker node not found' USING ERRCODE = 'P0002';
    END IF;

    RETURN QUERY SELECT v_node.id, v_token, v_node.token_version;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_revoke_worker_node(
    p_node_id uuid,
    p_reason text
)
RETURNS public.apocrypha_worker_node
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node public.apocrypha_worker_node;
BEGIN
    IF char_length(btrim(coalesce(p_reason, ''))) NOT BETWEEN 1 AND 500 THEN
        RAISE EXCEPTION 'revoke reason must contain 1-500 characters' USING ERRCODE = '23514';
    END IF;

    UPDATE public.apocrypha_worker_node AS worker_node
    SET status = 'revoked',
        revoked_at = coalesce(worker_node.revoked_at, now()),
        revoke_reason = btrim(p_reason),
        token_hash = public.apocrypha_sha256(
            'revoked:' || worker_node.id::text || ':' || worker_node.token_version::text || ':' || clock_timestamp()::text
        ),
        token_last_four = '0000',
        token_version = worker_node.token_version + 1
    WHERE worker_node.id = p_node_id
    RETURNING worker_node.* INTO v_node;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'worker node not found' USING ERRCODE = 'P0002';
    END IF;

    RETURN v_node;
END;
$$;
