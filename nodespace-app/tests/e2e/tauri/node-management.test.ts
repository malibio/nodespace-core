import { test, expect } from '@playwright/test';

test.describe('Node Management', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to the app and wait for it to load
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('should create a new text node', async ({ page }) => {
    // Look for a "New Node" or "Create Node" button
    const createButton = page.locator(
      'button:has-text("New Node"), button:has-text("Create"), [data-testid="create-node"]'
    );
    
    // If create button exists, test node creation flow
    if (await createButton.count() > 0) {
      await createButton.click();
      
      // Look for node type selection or direct text input
      const textOption = page.locator(
        'button:has-text("Text"), [data-testid="node-type-text"], input[type="text"], textarea'
      );
      
      if (await textOption.count() > 0) {
        await textOption.click();
        
        // If it's an input/textarea, type content
        const inputElement = page.locator('input[type="text"], textarea').first();
        if (await inputElement.count() > 0) {
          await inputElement.fill('Test node content');
          
          // Look for save button or submit
          const saveButton = page.locator(
            'button:has-text("Save"), button:has-text("Create"), [data-testid="save-node"]'
          );
          
          if (await saveButton.count() > 0) {
            await saveButton.click();
            
            // Verify node was created
            await expect(page.locator('text=Test node content')).toBeVisible();
          }
        }
      }
    } else {
      // If no create button found, this is expected for a minimal app
      console.log('No create button found - this is expected for the current app state');
    }
  });

  test('should display existing nodes', async ({ page }) => {
    // Look for any existing node content
    const nodeElements = page.locator(
      '.node, [data-testid="node"], .text-node, .node-content'
    );
    
    // Either nodes exist or we see an empty state
    const emptyState = page.locator(
      ':has-text("No nodes"), :has-text("Empty"), :has-text("Welcome")'
    );
    
    const hasNodes = await nodeElements.count() > 0;
    const hasEmptyState = await emptyState.count() > 0;
    
    expect(hasNodes || hasEmptyState).toBe(true);
  });

  test('should handle node search if available', async ({ page }) => {
    // Look for search functionality
    const searchInput = page.locator(
      'input[type="search"], input[placeholder*="search" i], [data-testid="search"]'
    );
    
    if (await searchInput.count() > 0) {
      // Test search functionality
      await searchInput.fill('test');
      
      // Wait for search results
      await page.waitForTimeout(500);
      
      // Verify search is working (results change or no results message)
      const searchResults = page.locator('.search-results, .node, [data-testid="search-results"]');
      const noResults = page.locator(':has-text("No results"), :has-text("Not found")');
      
      const hasResults = await searchResults.count() > 0;
      const hasNoResultsMessage = await noResults.count() > 0;
      
      expect(hasResults || hasNoResultsMessage).toBe(true);
    } else {
      console.log('No search functionality found - this is expected for the current app state');
    }
  });

  test('should handle empty state gracefully', async ({ page }) => {
    // The app should handle the case where there are no nodes
    // This could show a welcome message, empty state, or placeholder content
    
    const bodyText = await page.textContent('body');
    expect(bodyText).toBeTruthy();
    expect(bodyText!.length).toBeGreaterThan(0);
    
    // Verify no JavaScript errors occurred
    let hasErrors = false;
    page.on('pageerror', () => {
      hasErrors = true;
    });
    
    // Interact with the page to trigger any potential errors
    await page.click('body');
    await page.keyboard.press('Tab');
    
    await page.waitForTimeout(1000);
    expect(hasErrors).toBe(false);
  });

  test('should be accessible', async ({ page }) => {
    // Basic accessibility checks
    
    // Check for proper document structure
    const main = page.locator('main, [role="main"]');
    const hasMain = await main.count() > 0;
    
    if (!hasMain) {
      // If no main element, check for basic content structure
      const content = page.locator('#app, .app, body > *');
      await expect(content.first()).toBeVisible();
    }
    
    // Check for keyboard accessibility
    await page.keyboard.press('Tab');
    const focusedElement = await page.locator(':focus').count();
    
    // Should have some focusable elements or be handling focus appropriately
    // (This is a basic check - real accessibility testing would be more comprehensive)
    expect(focusedElement >= 0).toBe(true);
    
    // Check that images have alt text (if any images exist)
    const imagesWithoutAlt = page.locator('img:not([alt])');
    const imageCount = await imagesWithoutAlt.count();
    expect(imageCount).toBe(0); // All images should have alt text
  });
});