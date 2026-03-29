/**
 * Asset Utilities
 * Provides utility functions for asset operations including formatting,
 * validation, conversion, and comparison.
 */

import { Asset, AssetCode } from '../types/asset.types';
import { assetService } from '../services/asset.service';

/**
 * Format an asset code to a human-readable display name.
 * @param assetCode - The asset code (XLM, USDC, or PYUSD)
 * @returns The human-readable asset name (e.g., "USD Coin" for USDC)
 * @throws Error if the asset code is not supported
 */
export function formatAssetDisplay(assetCode: AssetCode): string {
  try {
    const asset = assetService.getAssetMetadata(assetCode);
    return asset.name;
  } catch (error) {
    throw new Error(`Cannot format display for asset: ${assetCode}`);
  }
}

/**
 * Get the path to an asset's icon image.
 * @param assetCode - The asset code (XLM, USDC, or PYUSD)
 * @returns The path to the asset's icon image
 * @throws Error if the asset code is not supported
 */
export function getAssetIcon(assetCode: AssetCode): string {
  if (!isValidAssetCode(assetCode)) {
    throw new Error(`Invalid asset code: ${assetCode}`);
  }

  const iconMap: Record<AssetCode, string> = {
    XLM: '/assets/icons/xlm.svg',
    USDC: '/assets/icons/usdc.svg',
    PYUSD: '/assets/icons/pyusd.svg',
  };

  return iconMap[assetCode];
}

/**
 * Validate if a code is a supported asset code.
 * @param code - The code to validate
 * @returns true if the code is a valid asset code (XLM, USDC, or PYUSD), false otherwise
 */
export function isValidAssetCode(code: string): boolean {
  return assetService.isValidAssetCode(code);
}

/**
 * Convert an amount from one asset to another using an exchange rate.
 * @param amount - The amount to convert
 * @param fromAsset - The source asset code
 * @param toAsset - The destination asset code
 * @param exchangeRate - The exchange rate (e.g., 0.0875 means 1 fromAsset = 0.0875 toAsset)
 * @returns The converted amount
 * @throws Error if amount is negative, exchange rate is invalid, or assets are invalid
 */
export function convertAmount(
  amount: number,
  fromAsset: AssetCode,
  toAsset: AssetCode,
  exchangeRate: number
): number {
  // Validate inputs
  if (amount < 0) {
    throw new Error('Amount cannot be negative');
  }

  if (exchangeRate <= 0) {
    throw new Error('Exchange rate must be positive');
  }

  if (!isValidAssetCode(fromAsset)) {
    throw new Error(`Invalid source asset: ${fromAsset}`);
  }

  if (!isValidAssetCode(toAsset)) {
    throw new Error(`Invalid destination asset: ${toAsset}`);
  }

  // Handle zero amount
  if (amount === 0) {
    return 0;
  }

  // If assets are the same, return the amount as-is
  if (fromAsset === toAsset) {
    return amount;
  }

  // Calculate converted amount
  const converted = amount * exchangeRate;

  // Check for precision loss or overflow
  if (!isFinite(converted)) {
    throw new Error('Conversion resulted in invalid number (overflow or underflow)');
  }

  return converted;
}

/**
 * Round an amount to the asset's decimal precision.
 * @param amount - The amount to round
 * @param assetCode - The asset code to determine precision
 * @returns The rounded amount
 * @throws Error if the asset code is not supported
 */
export function roundToAssetPrecision(amount: number, assetCode: AssetCode): number {
  try {
    const asset = assetService.getAssetMetadata(assetCode);

    // Handle edge cases
    if (!isFinite(amount)) {
      throw new Error('Amount must be a finite number');
    }

    // Round to the asset's decimal places
    const factor = Math.pow(10, asset.decimals);
    return Math.round(amount * factor) / factor;
  } catch (error) {
    if (error instanceof Error && error.message.includes('Unsupported asset')) {
      throw new Error(`Cannot round to precision for asset: ${assetCode}`);
    }
    throw error;
  }
}

/**
 * Compare two assets for equality.
 * @param asset1 - The first asset to compare
 * @param asset2 - The second asset to compare
 * @returns true if both assets are the same (same code and issuer), false otherwise
 */
export function compareAssets(asset1: Asset, asset2: Asset): boolean {
  // Compare asset codes
  if (asset1.code !== asset2.code) {
    return false;
  }

  // Compare issuers (both null for native asset, or exact match for non-native)
  if (asset1.issuer !== asset2.issuer) {
    return false;
  }

  return true;
}
