document.addEventListener('DOMContentLoaded', () => {
  const redirectElement = document.getElementById('login-success-redirect');
  if (!redirectElement) {
    return;
  }

  const redirectUrl = redirectElement.dataset.redirectUrl || '/gallery';
  const configuredDelay = Number.parseInt(redirectElement.dataset.redirectDelayMs || '', 10);
  const redirectDelayMs = Number.isFinite(configuredDelay) ? configuredDelay : 3000;

  window.setTimeout(() => {
    window.location.href = redirectUrl;
  }, redirectDelayMs);
});
