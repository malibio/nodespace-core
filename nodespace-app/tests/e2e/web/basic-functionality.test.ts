import { test, expect } from '@playwright/test';

test.describe('Web Version Basic Functionality', () => {
  test('should load web version successfully', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Verify page loaded
    await expect(page).toHaveTitle(/NodeSpace/);
    
    // Check for basic app structure
    const app = page.locator('#app, .app, main, [data-testid="app"]');
    await expect(app).toBeVisible();
  });

  test('should handle browser-specific features', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Test browser back/forward (if applicable)
    const currentUrl = page.url();
    
    // Navigate if there are links available
    const links = page.locator('a[href]');
    const linkCount = await links.count();
    
    if (linkCount > 0) {
      await links.first().click();
      await page.waitForTimeout(500);
      
      // Test browser back button
      await page.goBack();
      expect(page.url()).toBe(currentUrl);
    }
  });

  test('should work without Tauri-specific APIs', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Verify the app works without Tauri APIs
    // (In web mode, Tauri APIs won't be available)
    let consoleErrors: string[] = [];
    
    page.on('console', msg => {
      if (msg.type() === 'error') {
        consoleErrors.push(msg.text());
      }
    });
    
    // Interact with the app
    await page.click('body');
    await page.keyboard.press('Tab');
    await page.waitForTimeout(1000);
    
    // Filter out expected Tauri-related errors in web mode
    const unexpectedErrors = consoleErrors.filter(error => 
      !error.includes('__TAURI__') && 
      !error.includes('tauri') &&
      !error.toLowerCase().includes('tauri')
    );
    
    expect(unexpectedErrors.length).toBe(0);
  });

  test('should be mobile responsive', async ({ page }) => {
    // Test mobile viewports
    const mobileViewports = [
      { width: 375, height: 667 }, // iPhone SE
      { width: 414, height: 896 }, // iPhone XR
      { width: 360, height: 640 }, // Android
    ];
    
    for (const viewport of mobileViewports) {
      await page.setViewportSize(viewport);
      await page.goto('/');
      await page.waitForLoadState('networkidle');
      
      // Verify content is visible and accessible on mobile
      const app = page.locator('#app, .app, main, [data-testid="app"]');
      await expect(app).toBeVisible();
      
      // Check that content doesn't overflow horizontally
      const body = await page.locator('body').boundingBox();
      expect(body?.width).toBeLessThanOrEqual(viewport.width + 20); // Allow small tolerance
    }
  });

  test('should handle offline scenario gracefully', async ({ page, context }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Simulate offline
    await context.setOffline(true);
    
    // Try to interact with the app
    await page.click('body');
    await page.waitForTimeout(1000);
    
    // The app should still be functional for basic interactions
    // (This depends on the app's offline capabilities)
    const app = page.locator('#app, .app, main, [data-testid="app"]');
    await expect(app).toBeVisible();
    
    // Restore online
    await context.setOffline(false);
  });
});