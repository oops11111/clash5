# R14 UI reference

Tracking issue: https://github.com/oops11111/clash5/issues/69.

The user supplied `clash5-r14-ui-mockups.zip` on 2026-11-04.
SHA-256: `8fdd1afc330de0ab4be03f9a2ee65b0a348974bd6f41afd4ecedfd08df06a6ff`.
The 69 PNG, SVG, HTML and Markdown reference files are preserved under
[`r14-mockups`](r14-mockups/layout-island/README.md). Bundled Python generators
were not imported or executed. These are concept references, not implemented
screens or real node, latency or connection data.

Use the five horizontal tabs in this order: Home, Proxies, Subscriptions,
Plugins, Settings. This supersedes the earlier nine-entry sidebar. The island
layout places chains on Home, node inventory in Subscriptions, rules and the
single process-route editor in Proxies, and diagnostics/connections/logs under
Settings. Preserve existing capabilities and redirect legacy entry routes.

`layout-island/renders` and `layout-island/src` provide light/dark page
references. `renders` and `src` at the reference root cover confirmation,
error notices, empty states, skeletons, loading buttons and keyboard focus.
The implementation must use actual application state and translated strings;
sample names and green latency values in the images are illustrative only.
