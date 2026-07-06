interface VerifyResponse {
  authorized?: boolean;
  username?: string;
  is_admin?: boolean;
}

class UserMenu {
  private dropdown: HTMLElement;
  private toggleButton: HTMLButtonElement;
  private isOpen = false;
  private boundClickOutside = this.handleClickOutside.bind(this);

  constructor(root: HTMLElement) {
    const dropdown = root.querySelector<HTMLElement>('#userMenuDropdown');
    const toggleButton = root.querySelector<HTMLButtonElement>('.user-menu-toggle');

    if (!dropdown || !toggleButton) {
      throw new Error('User menu elements not found');
    }

    this.dropdown = dropdown;
    this.toggleButton = toggleButton;
    this.toggleButton.addEventListener('click', (event) => this.toggle(event));
    void this.loadUserInfo();
  }

  private toggle(event: Event): void {
    event.stopPropagation();
    this.isOpen ? this.close() : this.open();
  }

  private open(): void {
    this.isOpen = true;
    this.dropdown.classList.add('show');
    document.addEventListener('click', this.boundClickOutside);
  }

  private close(): void {
    this.isOpen = false;
    this.dropdown.classList.remove('show');
    document.removeEventListener('click', this.boundClickOutside);
  }

  private handleClickOutside(event: Event): void {
    const userMenu = this.dropdown.closest('.user-menu');
    if (userMenu && !userMenu.contains(event.target as Node)) {
      this.close();
    }
  }

  private async loadUserInfo(): Promise<void> {
    const contentDiv = document.getElementById('userMenuContent');
    if (!contentDiv) {
      return;
    }

    try {
      const response = await fetch('/api/verify');
      const data = await response.json() as VerifyResponse;

      if (data.authorized && data.username) {
        const adminLink = data.is_admin ? '<a href="/_admin">Admin</a>' : '';
        contentDiv.innerHTML = `
          <div class="user-info">
            Signed in as
            <span class="username">${this.escapeHtml(data.username)}</span>
          </div>
          ${adminLink}
          <a href="/_login/profile">Profile</a>
          <a href="/_login/logout">Sign out</a>
        `;
      } else {
        contentDiv.innerHTML = '<a href="/_login">Sign in</a>';
      }
    } catch (error) {
      console.error('Error checking auth status:', error);
      contentDiv.innerHTML = '<a href="/_login">Sign in</a>';
    }
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll<HTMLElement>('.user-menu').forEach((menu) => {
    new UserMenu(menu);
  });
});
