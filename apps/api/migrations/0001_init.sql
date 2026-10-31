-- 0001_init.sql — skema awal TranscribeAI
-- Lihat docs/04-database-design.md untuk ERD & penjelasan lengkap.

CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ===== Enum =====
CREATE TYPE transcript_status AS ENUM (
  'uploaded','queued','validating','chunking','transcribing',
  'processing','completed','failed','cancelled'
);
CREATE TYPE user_role AS ENUM ('user','admin');
CREATE TYPE user_plan AS ENUM ('free','pro');

-- ===== users =====
CREATE TABLE users (
  id uuid PRIMARY KEY,
  email citext NOT NULL UNIQUE,
  password_hash text NOT NULL,
  name text NOT NULL,
  avatar_url text,
  role user_role NOT NULL DEFAULT 'user',
  plan user_plan NOT NULL DEFAULT 'free',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz
);

-- ===== refresh_tokens =====
CREATE TABLE refresh_tokens (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash text NOT NULL UNIQUE,          -- SHA-256 dari token (docs/08 §1)
  expires_at timestamptz NOT NULL,
  revoked_at timestamptz,
  user_agent text,
  ip inet,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens (user_id);
CREATE INDEX idx_refresh_tokens_expires ON refresh_tokens (expires_at)
  WHERE revoked_at IS NULL;

-- ===== transcripts =====
CREATE TABLE transcripts (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  title text NOT NULL,
  source_filename text NOT NULL,
  size_bytes bigint NOT NULL,
  mime_type text NOT NULL,
  storage_key text NOT NULL,
  duration_seconds numeric,
  language varchar(8),                      -- hasil deteksi (ISO-639-1)
  requested_language varchar(8),
  model text NOT NULL,
  prompt_hint text,
  status transcript_status NOT NULL DEFAULT 'uploaded',
  progress smallint NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
  cancel_requested boolean NOT NULL DEFAULT false,
  error text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  completed_at timestamptz,
  deleted_at timestamptz
);

-- antrean worker: claim job tercepat (docs/04 §5)
CREATE INDEX idx_transcripts_queue ON transcripts (created_at)
  WHERE status = 'queued' AND deleted_at IS NULL;

-- listing dashboard user
CREATE INDEX idx_transcripts_user_created ON transcripts (user_id, created_at DESC)
  WHERE deleted_at IS NULL;

-- pencarian judul
CREATE INDEX idx_transcripts_title_trgm ON transcripts USING gin (title gin_trgm_ops);

-- ===== segments =====
CREATE TABLE segments (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  transcript_id uuid NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
  idx integer NOT NULL,
  start_ms bigint NOT NULL,
  end_ms bigint NOT NULL,
  text text NOT NULL,
  speaker varchar(64),                      -- v2: diarization
  confidence real,
  edited boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (transcript_id, idx)               -- sekaligus index (transcript_id, idx)
);

-- ===== transcript_events (audit trail ringkas) =====
CREATE TABLE transcript_events (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  transcript_id uuid NOT NULL REFERENCES transcripts(id) ON DELETE CASCADE,
  from_status transcript_status NOT NULL,
  to_status transcript_status NOT NULL,
  message text,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_transcript_events_transcript
  ON transcript_events (transcript_id, created_at);

-- ===== api_keys (skema disiapkan, dipakai mulai v2) =====
CREATE TABLE api_keys (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name text NOT NULL,
  key_hash text NOT NULL UNIQUE,
  prefix varchar(8) NOT NULL,               -- 8 karakter pertama, untuk identifikasi
  scopes text[] NOT NULL DEFAULT '{}',
  last_used_at timestamptz,
  revoked_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_api_keys_user ON api_keys (user_id);
