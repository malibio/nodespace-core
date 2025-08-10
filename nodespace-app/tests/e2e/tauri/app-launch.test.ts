import { test, expect } from '@playwright/test';

test.describe('Tauri App Launch', () => {
  test('should launch and display main window', async ({ page }) => {
    // Navigate to the Tauri app
    await page.goto('/');
    
    // Wait for the app to load
    await page.waitForLoadState('networkidle');
    
    // Check if the main app container is visible
    const appContainer = page.locator('[data-testid="main-app"], #app, main');
    await expect(appContainer).toBeVisible();
    
    // Check for basic UI elements (adjust selectors based on actual app structure)
    const title = page.locator('h1, [role="banner"], .app-title');
    await expect(title).toBeVisible();
  });

  test('should have proper window title', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Check window title
    await expect(page).toHaveTitle(/NodeSpace/);
  });

  test('should handle app navigation', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Test basic navigation (adjust based on actual app structure)
    // This is a placeholder test - update with actual navigation elements
    const navLinks = page.locator('nav a, [role="navigation"] a');
    
    if (await navLinks.count() > 0) {
      // Test first navigation link
      const firstLink = navLinks.first();
      await firstLink.click();
      
      // Verify navigation worked
      await page.waitForTimeout(1000); // Allow for navigation
      expect(page.url()).not.toBe('http://localhost:1420/');
    }
  });

  test('should be responsive', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Test different viewport sizes
    const viewports = [
      { width: 1920, height: 1080 }, // Desktop
      { width: 1366, height: 768 },  // Laptop
      { width: 768, height: 1024 },  // Tablet
    ];
    
    for (const viewport of viewports) {
      await page.setViewportSize(viewport);
      
      // Check that content is still visible and properly laid out
      const appContainer = page.locator('[data-testid="main-app"], #app, main');
      await expect(appContainer).toBeVisible();
      
      // Verify no horizontal scrollbar at desktop sizes
      if (viewport.width >= 1366) {
        const body = await page.locator('body').boundingBox();
        expect(body?.width).toBeLessThanOrEqual(viewport.width);
      }
    }
  });

  test('should handle keyboard navigation', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Test tab navigation
    await page.keyboard.press('Tab');
    
    // Check that focus is visible
    const focusedElement = page.locator(':focus');
    await expect(focusedElement).toBeVisible();
    
    // Test escape key (should not cause errors)
    await page.keyboard.press('Escape');
    
    // Verify app is still functional
    const appContainer = page.locator('[data-testid="main-app"], #app, main');
    await expect(appContainer).toBeVisible();
  });
});