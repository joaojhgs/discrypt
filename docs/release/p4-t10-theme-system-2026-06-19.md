# P4-T10 Theme System Evidence - 2026-06-19

## Scope

PER-40 implements frontend theme-system evidence for the Discord-like UI shell.
The change is limited to UI theme tokens, default palette alignment, focused
Playwright coverage, and release evidence documentation.

## Token Contract

The shadcn theme contract is defined in `apps/ui/src/app-config.ts` as
`shadcnThemeTokenNames`. Every registered theme must provide the full token set:

- `--background`
- `--foreground`
- `--card`
- `--card-foreground`
- `--popover`
- `--popover-foreground`
- `--primary`
- `--primary-foreground`
- `--secondary`
- `--secondary-foreground`
- `--muted`
- `--muted-foreground`
- `--accent`
- `--accent-foreground`
- `--destructive`
- `--destructive-foreground`
- `--border`
- `--input`
- `--ring`

The configured default is `graphite-calm`, and `apps/ui/src/styles.css` uses the
same palette for static boot CSS before React state is available. Runtime theme
application uses `createThemeStyle()` so shell layout variables can extend the
theme without weakening the required shadcn token set.

## Visual Evidence

Focused Playwright coverage in `apps/ui/tests/e2e/stateful-ui.spec.ts` verifies:

- the app shell starts with `data-theme="graphite-calm"`;
- every required shadcn token on the live shell matches the registered default;
- shell foreground/background and primary rail mark colors resolve from CSS
  variables, not separate hardcoded palette values;
- switching to `ocean-contrast` through the settings dialog updates the live
  shell token set; and
- narrow layout keeps the switched theme without horizontal overflow.

Expected screenshot artifacts from the focused spec:

- `theme-default-dark-desktop.png`
- `theme-ocean-contrast-narrow.png`

## Evidence Boundary

This is local frontend harness evidence only. It does not claim production
readiness for invite/admission, OpenMLS membership, verified presence, delivery,
voice route, transport, storage recovery, or governance behavior.
