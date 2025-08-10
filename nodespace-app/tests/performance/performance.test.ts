import { test, expect } from '@playwright/test';

test.describe('Performance Tests', () => {
  test('should load within acceptable time limits', async ({ page }) => {
    const startTime = Date.now();
    
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const loadTime = Date.now() - startTime;
    
    // App should load within 5 seconds
    expect(loadTime).toBeLessThan(5000);
    
    console.log(`App loaded in ${loadTime}ms`);
  });

  test('should handle large datasets efficiently', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Simulate creating/loading many nodes (if the UI supports it)
    const createButton = page.locator('button:has-text("New"), button:has-text("Create")');
    
    if (await createButton.count() > 0) {
      const startTime = Date.now();
      
      // Create multiple nodes rapidly
      for (let i = 0; i < 10; i++) {
        await createButton.click();
        
        // Fill in content if there's an input
        const input = page.locator('input, textarea').first();
        if (await input.count() > 0) {
          await input.fill(`Performance test node ${i}`);
          
          // Save if there's a save button
          const saveButton = page.locator('button:has-text("Save")');
          if (await saveButton.count() > 0) {
            await saveButton.click();
          }
        }
        
        await page.waitForTimeout(100); // Small delay between operations
      }
      
      const operationTime = Date.now() - startTime;
      console.log(`Created 10 nodes in ${operationTime}ms`);
      
      // Should complete within reasonable time (10 seconds for 10 operations)
      expect(operationTime).toBeLessThan(10000);
    } else {
      console.log('No create functionality found - skipping large dataset test');
    }
  });

  test('should maintain smooth scrolling performance', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Test scrolling performance
    const startY = 0;
    const endY = 1000;
    const scrollSteps = 10;
    const stepSize = (endY - startY) / scrollSteps;
    
    const startTime = Date.now();
    
    for (let i = 0; i < scrollSteps; i++) {
      await page.evaluate(y => window.scrollTo(0, y), startY + (stepSize * i));
      await page.waitForTimeout(50);
    }
    
    const scrollTime = Date.now() - startTime;
    console.log(`Scrolling completed in ${scrollTime}ms`);
    
    // Scrolling should be smooth (less than 2 seconds for full scroll)
    expect(scrollTime).toBeLessThan(2000);
  });

  test('should handle rapid user interactions', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const startTime = Date.now();
    
    // Perform rapid interactions
    for (let i = 0; i < 20; i++) {
      // Click on different areas
      await page.click('body', { position: { x: 100 + (i * 10), y: 100 + (i * 5) } });
      await page.keyboard.press('Tab');
      
      // Small delay to prevent overwhelming the system
      await page.waitForTimeout(25);
    }
    
    const interactionTime = Date.now() - startTime;
    console.log(`Rapid interactions completed in ${interactionTime}ms`);
    
    // Should handle interactions within reasonable time
    expect(interactionTime).toBeLessThan(3000);
    
    // Verify app is still responsive
    const app = page.locator('#app, .app, main, [data-testid="app"]');
    await expect(app).toBeVisible();
  });

  test('should have acceptable memory usage', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Get initial memory usage
    const initialMetrics = await page.evaluate(() => {
      if ('memory' in performance) {
        return (performance as any).memory;
      }
      return null;
    });
    
    if (initialMetrics) {
      console.log('Initial memory usage:', {
        used: Math.round(initialMetrics.usedJSHeapSize / 1024 / 1024) + 'MB',
        total: Math.round(initialMetrics.totalJSHeapSize / 1024 / 1024) + 'MB',
        limit: Math.round(initialMetrics.jsHeapSizeLimit / 1024 / 1024) + 'MB'
      });
      
      // Perform some operations to simulate usage
      for (let i = 0; i < 50; i++) {
        await page.click('body');
        await page.waitForTimeout(20);
      }
      
      // Check memory usage after operations
      const finalMetrics = await page.evaluate(() => {
        if ('memory' in performance) {
          return (performance as any).memory;
        }
        return null;
      });
      
      if (finalMetrics) {
        const memoryIncrease = finalMetrics.usedJSHeapSize - initialMetrics.usedJSHeapSize;
        const increaseInMB = memoryIncrease / 1024 / 1024;
        
        console.log('Final memory usage:', {
          used: Math.round(finalMetrics.usedJSHeapSize / 1024 / 1024) + 'MB',
          increase: Math.round(increaseInMB) + 'MB'
        });
        
        // Memory increase should be reasonable (less than 50MB for basic operations)
        expect(increaseInMB).toBeLessThan(50);
      }
    } else {
      console.log('Memory metrics not available in this browser');
    }
  });

  test('should handle network delays gracefully', async ({ page, context }) => {
    // Simulate slow network
    await context.route('**/*', route => {
      setTimeout(() => route.continue(), 100); // Add 100ms delay
    });
    
    const startTime = Date.now();
    
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const loadTimeWithDelay = Date.now() - startTime;
    console.log(`App loaded with network delay in ${loadTimeWithDelay}ms`);
    
    // Should still load within reasonable time even with network delays
    expect(loadTimeWithDelay).toBeLessThan(10000);
    
    // App should be functional despite delays
    const app = page.locator('#app, .app, main, [data-testid="app"]');
    await expect(app).toBeVisible();
  });
});