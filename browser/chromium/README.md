# Chromium website tracking

This Manifest V3 extension tracks active HTTP/HTTPS hostnames for Chrome, Edge, Brave, Opera, and Vivaldi. It sends domain-only, five-second samples to the installed Digital Wellbeing app over the browser's native-messaging pipe. The Windows app also checks that a supported browser process is actually foreground and the user is not idle before it saves a sample.

It does not request browser history, file access, page contents, or private-window access. Browser extensions are disabled in private/incognito windows by default; if you enable this extension there, the extension still rejects incognito windows. Non-web pages such as file URLs, browser settings, and extension pages are not recorded. Only hostnames are saved, with no paths, query strings, titles, or page contents.

## Install

1. Install Digital Wellbeing with DigitalWellbeing-Setup-x64.exe. The installer registers the native messaging host for Chrome and Edge for the current Windows user.
2. In the browser, open its extensions page and enable Developer mode.
3. Choose Load unpacked and select the browser/chromium directory inside the Digital Wellbeing install directory.
4. Keep the extension enabled. Website rows appear in Digital Wellbeing after active browsing samples arrive.

Brave, Opera, and Vivaldi support Chromium extensions, but their native-messaging registry discovery varies by vendor and release. The installer currently registers Chrome and Edge only. For the other Chromium browsers, install this extension manually and use the browser's documented native-messaging registration location if it does not discover the Chrome host registration.

## Privacy and limitations

The extension needs access to tab URLs to reduce them locally to hostnames. It does not persist URLs. Activity attribution is sampled when the active tab changes, the browser changes focus, and every five seconds; very brief tab changes may therefore be rounded to the nearest sample. Time in private windows counts only in the app's overall browser-process total, never toward a website.
