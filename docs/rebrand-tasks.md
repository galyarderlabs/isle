# Galyarder Labs Rebrand Tasks

This is the ordered execution checklist for
[rebrand-plan.md](rebrand-plan.md). All items are intentionally unchecked:
this commit records the plan only and does not rebrand the app.

## Phase 1: Decide and audit

### Task 1: Choose the product identity

**Description:** Settle the product name, short name, lowercase command/package
slug, reverse-DNS namespace, GNOME extension UUID, final repository slug, and
version sequence.

**Acceptance criteria:**
- [ ] Name, spelling, slug, namespace, repository URL, and version policy are
  recorded in `rebrand-plan.md`.
- [ ] The selected name has a trademark search recorded before public release.
- [ ] Decide whether the new app replaces the installed Isle instance or must
  coexist with upstream Isle; default recommendation is in-place replacement.

**Verification:** Review branding and namespace decisions together before code
changes. Confirm the repository slug and namespace are available to Galyarder
Labs.

- **Dependencies:** None
- **Files likely touched:** `docs/rebrand-plan.md`
- **Estimated scope:** Small

### Task 2: Inventory identity and provenance references

**Description:** Search tracked source, generated metadata, docs, package files,
assets, service files, and history. Classify references as replace, preserve
for attribution/history, third-party mark, or unrelated match.

**Acceptance criteria:**
- [ ] Inventory covers display strings, identifiers, URLs, binaries, services,
  settings/data paths, release tooling, docs, and artwork.
- [ ] Every upstream copyright/license notice and bundled asset notice has a
  keep/replace decision.
- [ ] Historical changelog and source-lineage references are distinguished
  from current product links.

**Verification:** Review the inventory against `git grep` searches for `Isle`,
`isle`, `zickrian`, old UUID/schema/D-Bus names, and old GitHub URLs.

- **Dependencies:** Task 1
- **Files likely touched:** `docs/rebrand-plan.md`, `docs/rebrand-tasks.md`
- **Estimated scope:** Medium

### Task 3: Define upgrade and data migration behavior

**Description:** Specify how the current installation moves to the new
extension UUID and namespace, including old extension shutdown, settings,
credentials, data directories, caches, and rollback.

**Acceptance criteria:**
- [ ] List every user state store and whether it stays at its current path or
  migrates.
- [x] Define a one-time settings migration that preserves values and types
      (`scripts/notchfolk-migrate-from-isle.sh`, documented in
      [migrate-from-isle.md](migrate-from-isle.md)).
- [ ] Define safe handling for keyring credentials and MCP client config.
- [ ] Define failure/rollback behavior and ensure no duplicate notch or daemon
  is left behind.

**Verification:** Walk through fresh install, upgrade from current Isle,
interrupted upgrade, and uninstall scenarios on paper before implementation.

- **Dependencies:** Tasks 1–2
- **Files likely touched:** `docs/rebrand-plan.md`, `docs/rebrand-tasks.md`
- **Estimated scope:** Medium

### Checkpoint: Decisions and audit

- [ ] Product name and namespace are approved.
- [ ] Identity inventory and license/provenance map are complete.
- [ ] Upgrade/migration strategy is approved before changing runtime IDs.

## Phase 2: Product-facing brand and docs

### Task 4: Prepare the new visual identity and product copy

**Description:** Design the new icon/logo and update visible product strings,
About page, metadata, screenshots, and support wording while preserving the
current Isle UI behavior.

**Acceptance criteria:**
- [ ] App name, icon, description, About page, and Settings titles use the new
  identity consistently.
- [ ] No screen suggests upstream endorsement; the About page gives factual
  source attribution.
- [ ] Existing third-party logos and assets retain their own attribution and
  allowed use.

**Verification:** Review closed/open notch, Settings, extension manager entry,
and screenshots at the target GNOME versions.

- **Dependencies:** Tasks 1–3
- **Files likely touched:** `gnome-extension/metadata.json`, icon assets,
  `gnome-extension/prefs.js`, selected copy in `README.md`
- **Estimated scope:** Medium

### Task 5: Rewrite current public documentation and links

**Description:** Update README, install/setup/update/uninstall guides, help,
MCP docs, and repository URLs. Retain historical references and attribution.

**Acceptance criteria:**
- [ ] Current instructions clone, install, update, report issues, and download
  releases from Galyarder Labs.
- [ ] No current command directs users to upstream's install script or release.
- [ ] Docs state GPL-3.0-or-later and identify the project as a modified
  derivative of Isle.

**Verification:** Search rendered/current docs for old install commands and
open every linked project/release URL.

- **Dependencies:** Tasks 1–2
- **Files likely touched:** `README.md`, `docs/install.md`, `docs/mcp.md`,
  `docs/dbus-api.md`, settings help strings
- **Estimated scope:** Medium

## Phase 3: Technical identity and compatibility

### Task 6: Migrate GNOME extension identity and settings

**Description:** Change the extension UUID, GSettings schema ID/path, metadata,
and icon names according to the approved namespace, with an in-place settings
migration.

**Acceptance criteria:**
- [ ] GNOME installs exactly one active branded extension after upgrade.
- [ ] Previous settings are copied with their original types and values.
- [ ] Reinstall, disable, and uninstall affect only the branded extension.

**Verification:** Install over the current version, compare all settings before
and after, log out/in, then disable/uninstall and confirm no orphaned extension.

- **Dependencies:** Tasks 1–3
- **Files likely touched:** `gnome-extension/metadata.json`, schema XML,
  `install.sh`, `uninstall.sh`, extension resource/icon manifests
- **Estimated scope:** Large; implement in a focused migration change

### Task 7: Migrate daemon, D-Bus, app, and service identifiers

**Description:** Rename application IDs, D-Bus names/object paths/interfaces,
service activation files, and systemd units where the approved coexistence
decision requires new identifiers.

**Acceptance criteria:**
- [ ] Extension, daemon, shelf, MCP, and installed services agree on the new
  names.
- [ ] Startup, restart, shutdown, and service activation work after upgrade.
- [ ] Any intentionally retained public D-Bus API is documented as a
  compatibility choice.

**Verification:** Check D-Bus ownership/introspection, daemon startup, shelf
activation, and service enable/disable across logout/login.

- **Dependencies:** Tasks 1–3 and Task 6
- **Files likely touched:** `crates/isle-daemon`, `crates/isle-shelf`,
  `gnome-extension/src/daemon.js`, `data/*.service`, installer files
- **Estimated scope:** Large; split by daemon and shelf if the mapping grows

### Task 8: Preserve data, credentials, and user commands

**Description:** Migrate or intentionally retain data/config/cache paths,
keyring service IDs, command names, MCP client config, and any scripts that
refer to the old project.

**Acceptance criteria:**
- [ ] Tasks, reminders, calendar data, water/calorie history, router keys,
  assistant history, and MCP clients remain available after upgrade.
- [ ] Credentials remain in the keyring and are never copied into plaintext
  files or logs.
- [ ] Old commands either have a compatibility wrapper or produce a clear
  migration message.

**Verification:** Upgrade with representative existing user data and verify
records/credentials through the app without exposing secrets in logs.

- **Dependencies:** Tasks 1–3 and Tasks 6–7
- **Files likely touched:** daemon store/keyring code, setup scripts, CLI tools,
  MCP scripts and docs
- **Estimated scope:** Large; keep file/data migration separate from UI copy

## Phase 4: Distribution and release

### Task 9: Rebrand installers, packaging, updates, and CI

**Description:** Update binary/package names, AUR metadata, release assets,
workflow paths, latest-version checks, shell install commands, and attestation
verification without replacing historical upstream release records.

**Acceptance criteria:**
- [ ] Clean install and update use only Galyarder Labs artifacts.
      (Started now: `get.sh` and the daemon's latest-version check point at
      `galyarderlabs/isle` — until that repository has Releases with the
      `isle-*-linux.tar.gz` assets, installs and update checks find nothing,
      which is safe: the checker just retries later.)
- [ ] Package upgrade/uninstall removes only files owned by the new package.
- [ ] Release workflow publishes source and correctly named artifacts from the
  standalone repository.

**Verification:** Run packaging and install/uninstall checks in a clean test
environment; verify GitHub release links and update notifications.

- **Dependencies:** Tasks 1–2 and Tasks 4–8
- **Files likely touched:** `install.sh`, `uninstall.sh`, `get.sh`, `scripts/`,
  `packaging/aur/`, `.github/workflows/`, update service code
- **Estimated scope:** Large; keep AUR/release changes in a separate commit

### Task 10: Final legal, search, and upgrade review

**Description:** Perform a scoped old-identity sweep, inspect the final diff,
verify all notices, and complete a real upgrade/release-candidate review.

**Acceptance criteria:**
- [ ] Every remaining old name/URL is intentionally retained and classified.
- [ ] GPL and third-party notices are intact; changed files clearly identify
  the modified project where required.
- [ ] Upgrade, settings migration, data access, rollback, and uninstall pass.
- [ ] New README and release identify the project as independent and preserve
  upstream provenance.

**Verification:** Run repository build/checks, package/install checks, GNOME
manual review, and link/name scans against the final candidate.

- **Dependencies:** Tasks 4–9
- **Files likely touched:** Review-driven follow-up only
- **Estimated scope:** Medium

### Checkpoint: Release candidate

- [ ] Existing installation upgrades without data loss or duplicate services.
- [ ] Fresh install, update, rollback, and uninstall work.
- [ ] Current docs and release links point to Galyarder Labs.
- [ ] License, attribution, and third-party asset notices pass review.
- [ ] Brand, namespace, and release version are approved before publication.
