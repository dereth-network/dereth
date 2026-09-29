-- Ported from ACE (ACEmulator), AGPL-3.0: Database/Base/AuthenticationBase.sql
-- (and the Entity Framework mapping in Source/ACE.Database/Models/Auth/AuthDbContext.cs).
--
-- empyrean-store authentication schema, version 1. Names, keys and the six access levels are ACE's.
-- Divergences from AuthenticationBase.sql (receipt 1.2 lists them all):
--   A1. `create_I_P_ntoa` and `last_Login_I_P_ntoa` (MySQL GENERATED ... VIRTUAL columns that
--       render the varbinary address as text) are dropped: nothing in ACE reads them.
--   A2. datetime columns are INTEGER: .NET DateTime ticks (100 ns since 0001-01-01, UTC), so a
--       value round-trips exactly. ACE's `create_Time` DEFAULT CURRENT_TIMESTAMP is dropped; ACE
--       always writes CreateTime itself (AuthenticationDatabase.CreateAccount).
--   A3. `accountName` is COLLATE utf8mb4_uca1400_ai_ci, the MariaDB server default the table takes
--       (registered on every connection by empyrean-store); the other text columns keep BINARY.

CREATE TABLE accesslevel (
    level  INTEGER NOT NULL DEFAULT 0 PRIMARY KEY,
    name   TEXT NOT NULL,
    prefix TEXT DEFAULT ''
);

CREATE TABLE account (
    accountId             INTEGER PRIMARY KEY AUTOINCREMENT,
    accountName           TEXT NOT NULL COLLATE utf8mb4_uca1400_ai_ci,  -- A3
    passwordHash          TEXT NOT NULL,                              -- base64 of the hash (bcrypt: the $2y$ string)
    passwordSalt          TEXT NOT NULL DEFAULT 'use bcrypt',         -- 'use bcrypt', or the legacy SHA512 salt
    accessLevel           INTEGER NOT NULL DEFAULT 0 REFERENCES accesslevel (level),
    email_Address         TEXT DEFAULT NULL,
    create_Time           INTEGER NOT NULL,                           -- A2
    create_I_P            BLOB DEFAULT NULL,
    last_Login_Time       INTEGER DEFAULT NULL,                       -- A2
    last_Login_I_P        BLOB DEFAULT NULL,
    total_Times_Logged_In INTEGER NOT NULL DEFAULT 0,
    banned_Time           INTEGER DEFAULT NULL,                       -- A2
    banned_By_Account_Id  INTEGER DEFAULT NULL,
    ban_Expire_Time       INTEGER DEFAULT NULL,                       -- A2
    ban_Reason            TEXT DEFAULT NULL
);
CREATE UNIQUE INDEX accountName_uidx ON account (accountName);
CREATE INDEX accesslevel_idx ON account (accessLevel);

INSERT INTO accesslevel (level, name, prefix) VALUES (0, 'Player', '');
INSERT INTO accesslevel (level, name, prefix) VALUES (1, 'Advocate', '');
INSERT INTO accesslevel (level, name, prefix) VALUES (2, 'Sentinel', 'Sentinel');
INSERT INTO accesslevel (level, name, prefix) VALUES (3, 'Envoy', 'Envoy');
INSERT INTO accesslevel (level, name, prefix) VALUES (4, 'Developer', '');
INSERT INTO accesslevel (level, name, prefix) VALUES (5, 'Admin', 'Admin');
