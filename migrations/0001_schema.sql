-- Bagerista: the whole database schema.
-- Money is stored as integer euro cents, dates as TEXT 'YYYY-MM-DD',
-- status codes in English (the Bulgarian labels live in src/models.rs).

-- ── People ───────────────────────────────────────────────────────────────
CREATE TABLE users (
    id             INTEGER PRIMARY KEY,
    email          TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    password_hash  TEXT    NOT NULL,                      -- argon2id PHC string
    full_name      TEXT    NOT NULL CHECK (length(full_name) BETWEEN 2 AND 100),
    phone          TEXT    NOT NULL,
    company_name   TEXT,                                  -- optional („Строй Инвест“ ЕООД)
    company_eik    TEXT    CHECK (company_eik IS NULL OR length(company_eik) IN (9, 13)),
    role           TEXT    NOT NULL DEFAULT 'client' CHECK (role IN ('client', 'admin')),
    created_at     TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- ── Catalogue ────────────────────────────────────────────────────────────
CREATE TABLE categories (
    id             INTEGER PRIMARY KEY,
    slug           TEXT    NOT NULL UNIQUE,               -- mini-bageri
    name           TEXT    NOT NULL,                      -- Мини багери
    description    TEXT    NOT NULL,
    undercarriage  TEXT    NOT NULL CHECK (undercarriage IN ('tracks', 'wheels')),
    sort_order     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE machines (
    id                           INTEGER PRIMARY KEY,
    slug                         TEXT    NOT NULL UNIQUE,         -- kubota-kx057-6
    category_id                  INTEGER NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
    name                         TEXT    NOT NULL,                -- „Kubota KX057-6“
    brand                        TEXT    NOT NULL,
    model                        TEXT    NOT NULL,
    year_made                    INTEGER NOT NULL CHECK (year_made BETWEEN 1990 AND 2100),
    weight_kg                    INTEGER NOT NULL CHECK (weight_kg > 0),
    dig_depth_mm                 INTEGER NOT NULL CHECK (dig_depth_mm > 0),
    width_mm                     INTEGER NOT NULL CHECK (width_mm > 0),
    engine_kw                    REAL    NOT NULL CHECK (engine_kw > 0),
    height_mm                    INTEGER NOT NULL CHECK (height_mm > 0),  -- for the spec plate and drawings
    bucket_l                     INTEGER NOT NULL CHECK (bucket_l > 0),
    price_per_day_cents          INTEGER NOT NULL CHECK (price_per_day_cents > 0),
    operator_price_per_day_cents INTEGER NOT NULL CHECK (operator_price_per_day_cents >= 0),
    engine_hours                 INTEGER NOT NULL DEFAULT 0 CHECK (engine_hours >= 0),
    service_interval_hours       INTEGER NOT NULL DEFAULT 500 CHECK (service_interval_hours > 0),
    last_service_hours           INTEGER NOT NULL DEFAULT 0
                                 CHECK (last_service_hours >= 0 AND last_service_hours <= engine_hours),
    description                  TEXT    NOT NULL,
    illustration                 TEXT    NOT NULL
                                 CHECK (illustration IN ('mini', 'midi', 'crawler', 'wheeled', 'backhoe')),
    active                       INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at                   TEXT    NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_machines_category ON machines(category_id, active);

CREATE TABLE machine_photos (
    id            INTEGER PRIMARY KEY,
    machine_id    INTEGER NOT NULL REFERENCES machines(id) ON DELETE CASCADE,
    content_type  TEXT    NOT NULL CHECK (content_type IN ('image/jpeg', 'image/png', 'image/webp')),
    data          BLOB    NOT NULL CHECK (length(data) <= 2097152),   -- 2 MB
    alt_text      TEXT    NOT NULL,                                    -- Bulgarian alt text
    sort_order    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_photos_machine ON machine_photos(machine_id, sort_order);

CREATE TABLE attachments (
    id                   INTEGER PRIMARY KEY,
    slug                 TEXT    NOT NULL UNIQUE,
    name                 TEXT    NOT NULL,                -- Хидравличен чук (малък)
    description          TEXT    NOT NULL,
    kind                 TEXT    NOT NULL CHECK (kind IN ('hammer', 'bucket', 'auger', 'ripper', 'grab', 'other')),
    price_per_day_cents  INTEGER NOT NULL CHECK (price_per_day_cents >= 0),
    active               INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1))
);

CREATE TABLE machine_attachments (                         -- which attachment fits which machine
    machine_id     INTEGER NOT NULL REFERENCES machines(id)    ON DELETE CASCADE,
    attachment_id  INTEGER NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
    PRIMARY KEY (machine_id, attachment_id)
);

-- ── Rentals ──────────────────────────────────────────────────────────────
CREATE TABLE bookings (
    id                INTEGER PRIMARY KEY,
    machine_id        INTEGER NOT NULL REFERENCES machines(id) ON DELETE RESTRICT,
    client_id         INTEGER NOT NULL REFERENCES users(id)    ON DELETE RESTRICT,
    start_date        TEXT    NOT NULL CHECK (date(start_date) IS start_date),   -- valid YYYY-MM-DD
    end_date          TEXT    NOT NULL CHECK (date(end_date) IS end_date),
    with_operator     INTEGER NOT NULL DEFAULT 0 CHECK (with_operator IN (0, 1)),
    delivery_km       INTEGER NOT NULL DEFAULT 0 CHECK (delivery_km BETWEEN 0 AND 150),
    delivery_address  TEXT,
    client_note       TEXT,
    status            TEXT    NOT NULL DEFAULT 'new'
                      CHECK (status IN ('new', 'approved', 'active', 'completed', 'declined', 'cancelled')),
    -- price snapshot (copied from services::pricing at request time; never recomputed)
    days              INTEGER NOT NULL CHECK (days >= 1),
    weekend_days      INTEGER NOT NULL CHECK (weekend_days >= 0),
    equipment_cents   INTEGER NOT NULL,   -- machine + attachments, after weekend rule
    discount_percent  INTEGER NOT NULL CHECK (discount_percent IN (0, 10, 15, 20)),
    discount_cents    INTEGER NOT NULL,
    operator_cents    INTEGER NOT NULL,
    transport_cents   INTEGER NOT NULL,
    net_cents         INTEGER NOT NULL,   -- без ДДС
    vat_cents         INTEGER NOT NULL,
    total_cents       INTEGER NOT NULL,   -- с ДДС
    -- operations
    admin_note        TEXT,
    hours_at_start    INTEGER,
    hours_at_end      INTEGER,
    created_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT    NOT NULL DEFAULT (datetime('now')),
    CHECK (end_date >= start_date),
    CHECK (julianday(end_date) - julianday(start_date) < 90),          -- max 90 days
    CHECK (delivery_km = 0 OR length(trim(coalesce(delivery_address, ''))) > 0),
    CHECK (hours_at_end IS NULL OR hours_at_start IS NULL OR hours_at_end >= hours_at_start)
);
CREATE INDEX idx_bookings_machine_dates ON bookings(machine_id, start_date, end_date);
CREATE INDEX idx_bookings_client        ON bookings(client_id, created_at);
CREATE INDEX idx_bookings_status        ON bookings(status, start_date);

CREATE TABLE booking_attachments (
    booking_id           INTEGER NOT NULL REFERENCES bookings(id)    ON DELETE CASCADE,
    attachment_id        INTEGER NOT NULL REFERENCES attachments(id) ON DELETE RESTRICT,
    price_per_day_cents  INTEGER NOT NULL,                -- price at the moment of the request
    PRIMARY KEY (booking_id, attachment_id)
);

-- ── Maintenance ──────────────────────────────────────────────────────────
CREATE TABLE service_records (
    id            INTEGER PRIMARY KEY,
    machine_id    INTEGER NOT NULL REFERENCES machines(id) ON DELETE CASCADE,
    service_date  TEXT    NOT NULL CHECK (date(service_date) IS service_date),
    engine_hours  INTEGER NOT NULL CHECK (engine_hours >= 0),
    kind          TEXT    NOT NULL CHECK (kind IN ('scheduled', 'repair', 'inspection')),
    description   TEXT    NOT NULL,
    cost_cents    INTEGER NOT NULL DEFAULT 0 CHECK (cost_cents >= 0),
    created_by    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_service_machine ON service_records(machine_id, service_date);

-- ── Infrastructure ───────────────────────────────────────────────────────
CREATE TABLE sessions (                                    -- used by session_store.rs
    id          TEXT    PRIMARY KEY,
    data        TEXT    NOT NULL,                          -- JSON of tower_sessions::Record
    expires_at  INTEGER NOT NULL                           -- unix seconds
);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);
