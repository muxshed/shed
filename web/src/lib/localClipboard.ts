// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.
// Works on insecure local-network HTTP addresses where navigator.clipboard is unavailable.
// Never send camera URLs or stream keys to a remote service.
export async function copyLocalText(text: string): Promise<'copied' | 'manual'> {
  if (!text) throw new Error('No connection address is available for this source.');

  if (typeof window !== 'undefined' && window.isSecureContext && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return 'copied';
    } catch {
      // Fall through to the direct user-gesture clipboard method.
    }
  }

  if (typeof document !== 'undefined') {
    const input = document.createElement('textarea');
    input.value = text;
    input.readOnly = true;
    input.setAttribute('aria-hidden', 'true');
    input.style.position = 'fixed';
    input.style.top = '0';
    input.style.left = '-9999px';
    input.style.opacity = '0';
    document.body.appendChild(input);
    let copied = false;
    try {
      input.focus();
      input.select();
      input.setSelectionRange(0, input.value.length);
      copied = document.execCommand('copy');
    } finally {
      input.remove();
    }
    if (copied) return 'copied';
  }

  // Guaranteed visible manual-copy option when a browser blocks clipboard writes.
  if (typeof window !== 'undefined') {
    window.prompt('Copy the address below with Ctrl+C, then close this box:', text);
  }
  return 'manual';
}
