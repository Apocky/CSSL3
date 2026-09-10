-- =====================================================================
-- § APOCRYPHA DIRECT SMS · service-role least-authority hardening
-- =====================================================================
-- The SMS application reaches storage only through the SECURITY DEFINER
-- RPCs defined by 0046. Supabase may grant broad table rights to
-- service_role by default, and GRANT statements do not narrow them.
-- Revoke direct storage access so evidence mutation remains RPC-bounded.
-- =====================================================================

REVOKE ALL ON TABLE public.apocrypha_sms_channels FROM service_role;
REVOKE ALL ON TABLE public.apocrypha_sms_messages FROM service_role;
REVOKE ALL ON TABLE public.apocrypha_sms_delivery_events FROM service_role;
REVOKE ALL ON SEQUENCE public.apocrypha_sms_delivery_events_id_seq FROM service_role;
