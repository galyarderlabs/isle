# Galyarder Labs Rebrand Plan

This plan covers the product rebrand of the standalone public repository
`galyarderlabs/isle`, whose settled final slug is `galyarderlabs/notchfolk`: the
rename is pending the formal trademark and domain check, and GitHub will
redirect the old name once it happens. The repository
is no longer a GitHub fork, but the app, docs, and runtime identifiers still
use Isle's original identity. The work is tracked in
[rebrand-tasks.md](rebrand-tasks.md).

**Status:** Planning only. No product branding or runtime identifiers have
been changed by this plan.

## Goals

- Give the project a distinct Galyarder Labs product identity across the app,
  settings, install/update flow, packages, and project docs.
- Preserve the current feature set, the user's installed settings and data, and
  a usable upgrade path from the installed Isle build.
- Keep the fork's source history and accurately credit the upstream project and
  other authors.
- Make the standalone repository and release links the source of truth for new
  installs and updates.

## Out of scope

- Reworking feature behavior or the notch design as part of the name change.
- Removing upstream copyright or licensing notices.
- Changing the project to a proprietary or more restrictive license.
- Deleting the personal fork or rewriting Git history.

## Current baseline

- Repository: public, standalone `galyarderlabs/isle` until the rename to the
  settled slug `galyarderlabs/notchfolk`; local `origin` points to it.
- Source lineage: based on [zickrian/isle](https://github.com/zickrian/isle),
  with the fork's accumulated UI, feature, and integration changes.
- License: `GPL-3.0-or-later` in `Cargo.toml` and the root `LICENSE`.
- The GNOME metadata still says `Isle`, uses
  `isle@zickrian.github.io`, and points at the upstream repository.
- Build, settings, service, D-Bus, keyring, update, and documentation strings
  still contain Isle/upstream identifiers in many files.
- The README identifies components under separate licenses, including Inter
  (OFL-1.1), bloub (MIT), Lucide (ISC), and the demo video (CC BY-SA 4.0).

## Settled decisions

- **Product name:** Notchfolk (lowercase slug `notchfolk`). The name reads as a
  group of small companions living in the notch, which matches the intended
  playful, always-with-you product direction rather than a single bot.
- **Tagline:** "A little world in your notch."
- **Repository description:** "A living desktop notch: music with synced
  lyrics, tasks, reminders, calendar, AI agents (Hermes, Claude Code, Codex,
  OMP), a built-in MCP server, AI router analytics, and subscription usage
  quota tracking. GNOME today — Windows and macOS coming."
- **Namespace:** `io.galyarderlabs.notchfolk` for the application ID and GNOME
  extension UUID, with identifiers on other platforms derived from the same
  namespace as support is added.
- **Platform direction:** GNOME first, with Windows and macOS support planned
  later. The brand and namespace are platform-neutral; GNOME-specific
  identifiers are implementation details of the GNOME build.
- **Install model:** in-place replacement of the current Isle installation
  with a one-time migration. Side-by-side installs are out of scope for the
  first release; they would require fully separate settings, data, services,
  and D-Bus names.
- **Public D-Bus API:** rename D-Bus well-known names, object paths, and
  interfaces together with the rest of the runtime identity. The D-Bus API is
  a Linux implementation detail, no external consumers have committed to the
  current names, and a future Windows build would not use D-Bus at all;
  `docs/dbus-api.md` is updated with the new names instead of being kept as a
  frozen compatibility surface.
- **Version policy:** start a fresh version sequence independent of upstream
  Isle releases, beginning at `1.0.0` for the first rebranded release, so the
  update check never treats the new release as a downgrade from an Isle
  version number.
- **Repository rename timing:** keep `galyarderlabs/isle` until a trademark
  and domain check on "Notchfolk" passes, then rename the repository before
  the first public rebranded release so that clone URLs, package recipes, and
  the update feed reference the final slug from day one.

A quick product search found no notch app named "Notchfolk", but that is not
a trademark clearance: a formal trademark search and domain availability check
are still required before public release (Task 1).

## Rebrand decisions to settle

1. **Attribution wording:** how the About page and README state that the
   project is a modified derivative of Isle by zickrian.

## License, provenance, and marks

The current project declares `GPL-3.0-or-later`. GPLv3 permits modified source
versions, while requiring notices that the work was modified and remains under
the license; conveying binaries also carries corresponding-source obligations
([GPLv3 §§5–6](https://www.gnu.org/licenses/gpl-3.0.html#section5),
[GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.en.html#GPLRequireSourcePostedPublic)).
Keep the root `LICENSE`, existing upstream copyright notices on inherited
files, and the separate notices for bundled assets. Add Galyarder Labs notices
only to new or modified contributions where appropriate.

The GPL does not grant rights to upstream trademarks. Use the new product name
and artwork for the app, and describe the source relationship factually in the
About page and README without suggesting upstream endorsement
([GPLv3 §7](https://www.gnu.org/licenses/gpl-3.0.html#section7)). The final
product name should receive a separate trademark search before release.

Rebranding does not revoke rights already granted under GPL or remove changes
already merged into upstream. It establishes a separate product identity and
future maintenance path; it does not make the source exclusive.

## Identity and migration strategy

Treat the rename as an explicit map, not a repository-wide blind replacement.
Classify every match as user-facing brand, owned technical identifier, upstream
provenance, historical release reference, third-party name, or unrelated text.
Preserve historical changelog references and license notices.

The technical identity inventory includes:

- GNOME extension UUID, metadata, GSettings schema ID/path, and icon names.
- D-Bus well-known names, object paths, interface names, and shelf app ID.
- systemd and D-Bus activation service names; installed binary and package
  names; installer and uninstaller constants.
- keyring service names, config/data/cache locations, MCP client config, and
  commands exposed to users.
- update API, download/install commands, GitHub URLs, release verification,
  AUR packaging, About/help links, and documentation examples.

The first implementation must define how the current installation upgrades.
Copy settings without losing key types or values; retain reminders, tasks,
calendar entries, water/calorie history, credentials, and MCP configuration;
disable/remove only the old extension instance owned by this installation;
and avoid two daemons or duplicate notches during transition. Keyring entries
are often bound to their service attribute: if the keyring service name
changes, the migration must explicitly copy or relabel existing items under
the new service name, not merely assume they will be found. Keep data paths
unchanged unless a separate migration is necessary and tested.

## Delivery phases

1. **Decide and audit:** choose brand/namespace/install model and classify old
   identity references, legal notices, and third-party assets.
2. **Public identity:** prepare the name, icon, metadata, About copy, README,
   install guide, and support/release links.
3. **Runtime migration:** update extension identity and settings migration,
   daemon/service/D-Bus IDs, keyring identity, and install/uninstall behavior in
   dependency order.
4. **Distribution:** update setup scripts, package metadata, update checks,
   release workflows, AUR recipe, and all current install instructions.
5. **Verify and release:** check old-name references, test upgrade and removal
   on the user's machine, run the repository's build/check suite, publish source
   and binaries under the new identity, and document the first rebranded
   release.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| A global replacement alters licenses, asset names, history, or unrelated text. | Use an explicit mapping over tracked text files, then review the diff and search each old identifier class. |
| A new extension UUID/schema appears as a second app and loses preferences. | Define in-place migration first; test upgrade, disable old UUID, and preserve settings values. |
| D-Bus/keyring/path changes break existing services or lose credentials. | Migrate each namespace with read-before-write and a rollback path; retain stable data paths where practical. |
| Old installer/update links keep installing upstream Isle. | Switch all current install/update surfaces together and verify generated commands point to Galyarder Labs. |
| Rebrand implies ownership of upstream work or trademarks. | Preserve provenance and third-party notices; use original product artwork and check the selected name separately. |
| Old and new packages conflict on the same machine. | Test package upgrade/uninstall and ensure each new package removes only its own files and services. |

## Completion criteria

- New product identity appears consistently in current app surfaces, docs,
  packages, install/update flows, and GitHub metadata.
- No current install/update path accidentally downloads from the old upstream.
- The existing local installation upgrades without losing settings, credentials,
  or Isle-owned data, and no duplicate extension/daemon remains.
- Upstream attribution, GPL-3.0-or-later, and third-party notices remain intact.
- The new release is reproducible from `galyarderlabs/notchfolk` and its source
  is available under the declared license.
