import { DomUtils } from '../../core/dom-utils.js';
import type { MasonryConfig } from '../../core/types.js';

export class MasonryLayout {
  private container: HTMLElement;
  private items: HTMLElement[] = [];
  private columnCount: number = 1;
  private config: MasonryConfig;
  private resizeObserver?: ResizeObserver;
  private isLayoutPending = false;

  constructor(container: HTMLElement, config: Partial<MasonryConfig> = {}) {
    this.container = container;
    this.config = {
      gap: 20,
      breakpoints: { 768: 1, 1024: 2, 1440: 3 },
      minColumnWidth: 300,
      ...config
    };
    
    this.updateItems();
    this.calculateColumns();
    this.setupResizeObserver();
    this.layout();
  }

  private updateItems(): void {
    this.items = Array.from(
      this.container.querySelectorAll('.gallery-item')
    ) as HTMLElement[];
  }

  private calculateColumns(): void {
    const containerWidth = this.container.offsetWidth;
    const breakpoints = Object.entries(this.config.breakpoints)
      .sort(([a], [b]) => Number(b) - Number(a));
    
    for (const [width, cols] of breakpoints) {
      if (containerWidth >= Number(width)) {
        this.columnCount = cols;
        return;
      }
    }
    
    // Fallback: calculate based on min column width
    this.columnCount = Math.floor(containerWidth / this.config.minColumnWidth) || 1;
  }

  public async layout(): Promise<void> {
    if (this.isLayoutPending) return;
    this.isLayoutPending = true;

    // Wait for images to load
    await DomUtils.waitForImages(this.container);
    
    const columnHeights = new Array(this.columnCount).fill(0);
    const columnWidth = (100 / this.columnCount);
    
    this.items.forEach((item) => {
      const shortestColumnIndex = columnHeights.indexOf(Math.min(...columnHeights));
      const x = shortestColumnIndex * columnWidth;
      const y = columnHeights[shortestColumnIndex];
      
      // Apply transform with proper units
      item.style.transform = `translate3d(${x}%, ${y}px, 0)`;
      item.style.width = `${columnWidth}%`;
      item.style.position = 'absolute';
      
      // Update column height
      const itemHeight = item.offsetHeight || 200; // fallback height
      columnHeights[shortestColumnIndex] += itemHeight + this.config.gap;
    });
    
    // Set container height
    const maxHeight = Math.max(...columnHeights);
    this.container.style.height = `${maxHeight}px`;
    this.container.style.position = 'relative';
    
    this.isLayoutPending = false;
  }

  private setupResizeObserver(): void {
    if (!window.ResizeObserver) return;

    this.resizeObserver = new ResizeObserver(
      DomUtils.debounce(() => {
        const oldColumnCount = this.columnCount;
        this.calculateColumns();
        
        if (oldColumnCount !== this.columnCount) {
          this.layout();
        }
      }, 250)
    );

    this.resizeObserver.observe(this.container);
  }

  public addItems(newItems: HTMLElement[]): void {
    this.items.push(...newItems);
    this.layout();
  }

  public refresh(): void {
    this.updateItems();
    this.layout();
  }

  public destroy(): void {
    this.resizeObserver?.disconnect();
  }

  public getColumnCount(): number {
    return this.columnCount;
  }

  public getConfig(): MasonryConfig {
    return { ...this.config };
  }
}