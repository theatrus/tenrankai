# E2E tests (Playwright)

End-to-end browser tests that boot the real Rust server against an isolated
fixture gallery and verify **gallery display** and **image ordering** — the
areas that have regressed before.

## What it covers

- `tests/ordering.spec.ts` — filename asc/desc and custom sort orders render in
  the configured sequence; the rendered DOM order matches the server API order.
- `tests/display.spec.ts` — the square grid renders every image, thumbnails are
  served without errors, and clicking an image opens its detail page.

Ordering is asserted in **square-grid** mode (`grid_mode = "square"` in each
folder's `_folder.md`), because masonry distributes images across columns and
the DOM order is no longer the visual order.

## Fixtures

- `fixtures/config.toml` + `fixtures/config.d/` — a minimal single-gallery site
  (`/g`) using `image_indexing = "filename"`.
- `fixtures/photos/<folder>/_folder.md` — committed; sets `grid_mode` and the
  sort config per folder.
- Image bytes are **generated at test time** by `global-setup.ts` (a
  dependency-free PNG writer), so no binaries are committed and ordering is
  deterministic.

## Running

```bash
npm run build          # build frontend assets the server serves
npx playwright install # one-time: install the chromium browser
npm run test:e2e
```

The Playwright `webServer` builds and launches the server with
`cargo run --no-default-features -- serve --config e2e/fixtures/config.toml`.
First run compiles the backend, so allow a few minutes.

## Screenshots

Each scenario writes a full-page screenshot to `e2e/screenshots/` (e.g.
`ordering-custom.png`, `display-grid.png`) so results can be eyeballed. Failure
screenshots and traces land in `e2e/test-results/`; open the HTML report with
`npx playwright show-report` (under `e2e/`).
