# Galyarder Labs Rebrand Plan

This plan covers the future product rebrand of the standalone public repository
[`galyarderlabs/isle`](https://github.com/galyarderlabs/isle). The repository
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

- Repository: public, standalone `galyarderlabs/isle`; local `origin` points to
  this repository.
- Source lineage: based on [zickrian/isle](https://github.com/zickrian/isle),
  with the fork's accumulated UI, feature, and integration changes.
- License: `GPL-3.0-or-later` in `Cargo.toml` and the root `LICENSE`.
- The GNOME metadata still says `Isle`, uses
  `isle@zickrian.github.io`, and points at the upstream repository.
- Build, settings, service, D-Bus, keyring, update, and documentation strings
  still contain Isle/upstream identifiers in many files.
- The README identifies components under separate licenses, including Inter
  (OFL-1.1), bloub (MIT), Lucide (ISC), and the demo video (CC BY-SA 4.0).

## Rebrand decisions to settle

1. **Product name and spelling:** display name, capitalization, short name, and
   a pronounceable CLI/package slug.
2. **Namespace:** a stable reverse-DNS application ID and GNOME extension UUID
   controlled by Galyarder Labs.
3. **Install model:** replace the current Isle installation in place, or allow
   the new app to run beside upstream Isle. The recommended first release is an
   in-place replacement with a one-time migration; side-by-side installs need
   fully separate settings, data, services, and D-Bus names.
4. **Repository and release identity:** final GitHub slug, homepage, issue
   tracker, release tag sequence, package names, and update feed. The existing
   `galyarderlabs/isle` repository can be renamed after the product slug is
   chosen.
5. **Attribution wording:** how the About page and README state that the
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
and avoid two daemons or duplicate notches during transition. Keep data paths
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
- The new release is reproducible from `galyarderlabs/isle` and its source is
  available under the declared license.
