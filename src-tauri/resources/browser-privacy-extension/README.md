# clash5 Browser Privacy companion

This Chromium extension pairs with clash5's bundled IPv6/DNS guard. The `.c5p` plugin controls Mihomo configuration; this extension controls browser-visible regional values. They are separate trust boundaries. Neither component changes the other's settings.

## Install locally

1. Open `chrome://extensions` or `edge://extensions`, enable Developer mode, and choose **Load unpacked**.
2. Select this `src-tauri/resources/browser-privacy-extension` directory in a source checkout, or the extension directory shown on clash5's Installed Plugins page in an installed build. No store account is needed.
3. Open the extension and add a hostname with the New York or Tokyo profile. Reload existing tabs.
4. To remove a profile, remove its rule and reload. An explicit **Browser default** rule for a subdomain excludes it from its parent rule.

The extension asks for HTTP/HTTPS host access because Chromium needs permission to apply a rule at document start. No rule means no page modification. Rules stay in `chrome.storage.local`; they are not sent to clash5 or a server.

## Check

Run `python -m http.server 8765` from this directory, add `localhost` to the extension with a profile, then visit `http://localhost:8765/verify.html`. Check the language, Intl time zone, and Date offset. In DevTools Network, inspect the request's `Accept-Language` header. Change the profile and reload. Finally remove the rule and reload to confirm browser defaults return. `node --test policy.test.mjs` checks matching and exclusion logic.

The separate [Who Are You](https://github.com/yt1988/who-are-you) site can give a broader, optional external check. Visiting it sends your IP and browser signals to that service and other test endpoints; the extension never opens it automatically.

## Limits

The profiles change `navigator.language(s)`, default `Intl.DateTimeFormat` language/time zone, local Date getters, Date locale formatting, and `Accept-Language` for requests associated with a matching site. Other signals can disagree: `Date.toString()`, explicit locale choices, Worker contexts, WebRTC, UA/Client Hints, screen, fonts, Canvas, WebGL, and audio are untouched. IP, DNS, TUN routing, cookies, and account identity are not changed. This is a regional fingerprint switch, not a complete browser persona or a guarantee against tracking.

Only HTTP/HTTPS documents are covered. Existing pages need a reload. The two Chrome APIs that apply scripts and headers cannot update atomically; wait for the save confirmation before opening a new page. For network privacy, separately enable and verify clash5's IPv6/DNS guard and route configuration.

The implementation is original. [browser-fingerprint-switch](https://github.com/kotone/browser-fingerprint-switch) informed the scope; its repository does not declare a license, so no source was copied. [Who Are You](https://github.com/yt1988/who-are-you) informed the check list.
