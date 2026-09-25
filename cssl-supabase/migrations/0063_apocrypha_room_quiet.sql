-- 0063 · mute means "Apocrypha does not speak unprompted in this room" (owner steering 2026-09-25:
-- muting used to hide past unprompted rows instead of stopping them). The local loop reads this
-- flag before free speech; history is never hidden.
ALTER TABLE public.apocrypha_room ADD COLUMN IF NOT EXISTS quiet boolean NOT NULL DEFAULT false;
