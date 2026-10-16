# Chromium website tracking

This Manifest V3 extension tracks active HTTP/HTTPS hostnames for Chrome, Edge, Brave, Opera, and Vivaldi. It sends domain-only, five-second samples to the installed Digital Wellbeing app over the browser's native-messaging pipe. The Windows app also checks that a supported browser process is actually foreground and the user is not idle before it saves a sample.

It does not request browser history, file access, page contents, or private-window access. Browser extensions are disabled in private/incognito windows by default; if you enable this extension there, the extension still rejects incognito windows. Non-web pages such as file URLs, browser settings, and extension pages are not recorded. Only hostnames are saved, with no paths, query strings, titles, or page contents.

## Install

1. Install Digital Wellbeing with DigitalWellbeing-Setup-x64.exe. The installer registers the native messaging host for Chrome/Opera and Edge for the current Windows user.
2. In Opera GX, open `opera://extensions` and enable Developer mode. Chrome and Edge use their own extensions pages.
3. Choose Load unpacked and select the browser/chromium directory inside the Digital Wellbeing install directory.
4. Keep the extension enabled. Website rows appear in Digital Wellbeing after active browsing samples arrive.

Opera uses Chrome's Windows native-messaging registration location ([Opera native messaging documentation](https://help.opera.com/en/extensions/message-passing/#native-messaging)). No separate Opera registry entry is needed. The app's **Set up website tracking** page opens Opera GX's extensions page and the installed extension folder. Loading the extension is a one-time browser step; installing the desktop app alone does not install it into your browser. Brave and Vivaldi have not been verified.

## Privacy and limitations

The extension needs access to tab URLs to reduce them locally to hostnames. It does not persist URLs. Activity attribution is sampled when the active tab changes, the browser changes focus, and every five seconds; very brief tab changes may therefore be rounded to the nearest sample. Only a focused, non-minimized browser window is attributed to a website. The native host also checks the exact launching browser process, Windows idle/lock state, and the app's tracking pause setting. Time in private windows counts only in the app's overall browser-process total, never toward a website. Website time is a breakdown of browser time and is not added again to the overall screen-time total.
