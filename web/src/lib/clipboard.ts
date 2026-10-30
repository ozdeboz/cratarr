// Copying text: the Clipboard API where the page is secure (https, localhost); elsewhere (Cratarr at a home address
// over http, as the app and the LAN dev server reach it) that API is not there, and a hidden field's text is copied
// the older way, which browsers and Android's WebView still allow there.

/** Copies `text`; false when nothing could be copied. */
export async function copyText(text: string): Promise<boolean> {
  if (window.isSecureContext && navigator.clipboard) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      /* refused (no permission): the older way below */
    }
  }
  const field = document.createElement('textarea');
  field.value = text;
  field.setAttribute('readonly', '');
  field.style.position = 'fixed';
  field.style.opacity = '0';
  field.style.pointerEvents = 'none';
  document.body.appendChild(field);
  field.select();
  let done = false;
  try {
    done = document.execCommand('copy');
  } catch {
    done = false;
  }
  field.remove();
  return done;
}
