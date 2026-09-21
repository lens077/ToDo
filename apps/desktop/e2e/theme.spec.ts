import { test, expect } from '@playwright/test';

/**
 * The task in flight sits on tinted paper, never a dark block: in the light
 * scheme its surface stays close to the page, and in both schemes the ink on
 * it clears AA body contrast against the darkest stop of the tint.
 */
for (const scheme of ['light', 'dark'] as const) {
  test(`${scheme} focus card is tinted paper with readable ink`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: 'reduce' });
    await page.goto('/');
    const card = page.locator('.task-card--focus').first();
    await expect(card).toBeVisible();
    const colors = await card.evaluate((element) => {
      const style = getComputedStyle(element);
      const stops = [...style.backgroundImage.matchAll(/rgb\((\d+), (\d+), (\d+)\)/g)]
        .map((match) => match.slice(1).map(Number));
      const luminance = (rgb: number[]) => rgb.map((v) => {
        const s = v / 255;
        return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      }).reduce((sum, channel, index) => sum + channel * [0.2126, 0.7152, 0.0722][index], 0);
      const ink = style.color.match(/\d+/g)!.map(Number).slice(0, 3);
      const lums = stops.map(luminance);
      const inkLum = luminance(ink);
      const contrastAgainst = (l: number) => (Math.max(inkLum, l) + 0.05) / (Math.min(inkLum, l) + 0.05);
      return {
        background: style.backgroundImage,
        color: style.color,
        brightest: Math.max(...lums),
        darkest: Math.min(...lums),
        contrast: Math.min(...lums.map(contrastAgainst)),
      };
    });
    expect(colors.contrast).toBeGreaterThan(4.5);
    if (scheme === 'light') {
      // Tinted paper: the surface stays bright, the ink is the page ink.
      expect(colors.darkest).toBeGreaterThan(0.8);
      expect(colors.background).toBe('linear-gradient(rgb(255, 248, 236) 0%, rgb(253, 241, 220) 100%)');
      expect(colors.color).toBe('rgb(27, 21, 71)');
    } else {
      // Night: one step above the panel, never a bright block.
      expect(colors.brightest).toBeLessThan(0.06);
      expect(colors.background).toContain('rgb(42, 35, 64)');
      expect(colors.color).toBe('rgb(236, 234, 247)');
    }
  });
}
