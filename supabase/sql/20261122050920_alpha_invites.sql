-- Alpha download invite codes. Only SHA-256 hashes of codes are stored. Supabase-only, not in the repo migrations.
CREATE TABLE public.alpha_invites (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  code_hash text NOT NULL UNIQUE CHECK (code_hash ~ '^[0-9a-f]{64}$'),
  label text CHECK (label IS NULL OR char_length(label) <= 64),
  max_downloads integer NOT NULL DEFAULT 3 CHECK (max_downloads BETWEEN 1 AND 100),
  downloads integer NOT NULL DEFAULT 0 CHECK (downloads >= 0),
  revoked boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz,
  last_used_at timestamptz,
  CHECK (downloads <= max_downloads)
);
ALTER TABLE public.alpha_invites ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON public.alpha_invites FROM anon, authenticated;
