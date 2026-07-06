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
        contentDiv.replaceChildren(...this.createSignedInMenu(data.username, Boolean(data.is_admin)));
      } else {
        contentDiv.replaceChildren(this.createLink('/_login', 'Sign in'));
      }
    } catch (error) {
      console.error('Error checking auth status:', error);
      contentDiv.replaceChildren(this.createLink('/_login', 'Sign in'));
    }
  }

  private createSignedInMenu(username: string, isAdmin: boolean): HTMLElement[] {
    const userInfo = document.createElement('div');
    userInfo.className = 'user-info';
    userInfo.append('Signed in as');

    const usernameElement = document.createElement('span');
    usernameElement.className = 'username';
    usernameElement.textContent = username;
    userInfo.append(usernameElement);

    return [
      userInfo,
      ...(isAdmin ? [this.createLink('/_admin', 'Admin')] : []),
      this.createLink('/_login/profile', 'Profile'),
      this.createLink('/_login/logout', 'Sign out'),
    ];
  }

  private createLink(href: string, text: string): HTMLAnchorElement {
    const link = document.createElement('a');
    link.href = href;
    link.textContent = text;
    return link;
  }
}

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll<HTMLElement>('.user-menu').forEach((menu) => {
    new UserMenu(menu);
  });
});
