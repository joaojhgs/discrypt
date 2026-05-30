# G130 UI integration gate


Design read: dark, Discord-like product shell for technical users, with a dense
but usable command center, shadcn-owned components, configurable themes, and
honest production state instead of pixel-art polish or mocked members.

G130 closes the production UI integration story. The gate verifies that the React
surface exposes the completed backend capabilities through owned shadcn-style
components, not raw ad-hoc controls, and that local-dev harness behavior remains
honest and tested.

The gate checks:

- setup and recovery commands are reachable from the first-run shell;
- DMs, groups, joining, invites, channels, text send, voice join/leave, mute,
  speaker volume, safety verification, preferences, and reset all remain wired to
  the strict Tauri command clients;
- the main UI uses the local shadcn component set for buttons, cards, inputs,
  labels, selects, scroll areas, sliders, switches, badges, avatars, and
  separators;
- the configurable theme/template system remains in `apps/ui/src/app-config.ts`;
- Playwright specs cover setup/recovery, DM, group invite/join, channel creation,
  text send, voice controls, fake-member absence, responsive navigation,
  persistence, and transport-status honesty; and
- CI runs `test:ui-integration-g130`.

Run:

```sh
npm --prefix apps/ui run test:ui-integration-g130
npm --prefix apps/ui run test:e2e
```
