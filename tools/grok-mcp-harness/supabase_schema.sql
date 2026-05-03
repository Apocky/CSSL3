-- Supabase Schema for Grok MCP Harness Tool Call Logging
-- Run this in your Supabase SQL editor (or via migration)

-- Enable UUID extension if not already
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Main logging table
CREATE TABLE IF NOT EXISTS public.tool_calls (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    tool_name TEXT NOT NULL,
    args JSONB,
    result JSONB,
    success BOOLEAN DEFAULT true,
    duration_ms INTEGER,
    project TEXT,                    -- e.g. 'labyrinth', 'akashic', 'infinity-engine', 'csl', 'cssl'
    user_id UUID,                    -- Link to auth.users if using Supabase Auth
    session_id TEXT,                 -- Optional: group calls from same Grok conversation
    ip_address INET,
    error_message TEXT
);

-- Indexes for fast querying
CREATE INDEX IF NOT EXISTS idx_tool_calls_created_at ON public.tool_calls(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_calls_tool_name ON public.tool_calls(tool_name);
CREATE INDEX IF NOT EXISTS idx_tool_calls_project ON public.tool_calls(project);
CREATE INDEX IF NOT EXISTS idx_tool_calls_success ON public.tool_calls(success);

-- Optional: Enable Row Level Security (recommended)
ALTER TABLE public.tool_calls ENABLE ROW LEVEL SECURITY;

-- Example policy: Only service role or specific users can insert/read
-- (Adjust according to your Supabase setup)
CREATE POLICY "Allow service role full access" ON public.tool_calls
    FOR ALL USING (auth.role() = 'service_role');

-- Helpful view for recent activity
CREATE OR REPLACE VIEW public.recent_tool_activity AS
SELECT 
    created_at,
    tool_name,
    project,
    success,
    duration_ms,
    (args->>'spec_type') as spec_type,
    (result->>'m2') as m2_score
FROM public.tool_calls
ORDER BY created_at DESC
LIMIT 100;

-- Function to log a tool call (call this from harness.py via Supabase client)
CREATE OR REPLACE FUNCTION public.log_tool_call(
    p_tool_name TEXT,
    p_args JSONB DEFAULT NULL,
    p_result JSONB DEFAULT NULL,
    p_success BOOLEAN DEFAULT true,
    p_duration_ms INTEGER DEFAULT NULL,
    p_project TEXT DEFAULT NULL,
    p_session_id TEXT DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_id UUID;
BEGIN
    INSERT INTO public.tool_calls (
        tool_name, args, result, success, duration_ms, project, session_id
    ) VALUES (
        p_tool_name, p_args, p_result, p_success, p_duration_ms, p_project, p_session_id
    ) RETURNING id INTO new_id;
    
    RETURN new_id;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON TABLE public.tool_calls IS 'Audit log for all Grok MCP Harness tool invocations on Apocky projects';
