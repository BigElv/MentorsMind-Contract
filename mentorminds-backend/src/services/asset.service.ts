/**
 * Asset Service
 * Centralized service for managing asset metadata and operations.
 * Provides methods to query asset information, validate asset codes,
 * and format/parse amounts to asset precision.
 */

import { Asset, AssetCode } from '../types/asset.types';

/**
 * Asset metadata for supported assets on the Stellar network.
 * Includes XLM (native), USDC, and PYUSD with their respective
 * issuers, decimal precision, and display names.
 */
const ASSETS: Record<AssetCode, Asset> = {
  XLM: {
    code: 'XLM',
    issuer: null, // Native asset
    decimals: 7,
    name: 'Stellar Lumens',
  },
  USDC: {
    code: 'USDC',
    issuer: 'GBBD47UZQ2BNSE7E2CMML7BNPI5BEFF2KE5FIXEDISSUERADDRESS',
    decimals: 6,
    name: 'USD Coin',
  },
  PYUSD: {
    code: 'PYUSD',
    issuer: 'GDZ55LVXECRTW4G36ICJVWCIHL7BQUM2FixedIssuerAddress',
    decimals: 6,
    name: 'PayPal USD',
  },
};

/**
 * AssetService class
 * Provides centralized asset metadata management and operations.
 */
class AssetService {
  /**
   * Get metadata for a specific asset.
   * @param assetCode - The asset code (XLM, USDC, or PYUSD)
   * @returns The asset metadata including code, issuer, decimals, and name
   * @throws Error if the asset code is not supported
   */
  getAssetMetadata(assetCode: AssetCode): Asset {
    const asset = ASSETS[assetCode];
    if (!asset) {
      throw new Error(`Unsupported asset code: ${assetCode}`);
    }
    return asset;
  }

  /**
   * Check if an asset is the native asset (XLM).
   * @param assetCode - The asset code to check
   * @returns true if the asset is XLM (native), false otherwise
   */
  isNativeAsset(assetCode: AssetCode): boolean {
    return assetCode === 'XLM';
  }

  /**
   * Format an amount to the asset's decimal precision.
   * @param amount - The amount to format as a number
   * @param assetCode - The asset code to determine precision
   * @returns The formatted amount as a string with proper decimal places
   * @throws Error if the asset code is not supported
   */
  formatAssetAmount(amount: number, assetCode: AssetCode): string {
    const asset = this.getAssetMetadata(assetCode);
    return amount.toFixed(asset.decimals);
  }

  /**
   * Parse a string amount to a number with proper decimal handling.
   * @param amountString - The amount string to parse
   * @param assetCode - The asset code to validate precision
   * @returns The parsed amount as a number
   * @throws Error if the asset code is not supported or the string is invalid
   */
  parseAssetAmount(amountString: string, assetCode: AssetCode): number {
    const asset = this.getAssetMetadata(assetCode);
    const parsed = parseFloat(amountString);
    
    if (isNaN(parsed)) {
      throw new Error(`Invalid amount string: ${amountString}`);
    }
    
    return parsed;
  }

  /**
   * Get a list of all supported assets.
   * @returns Array of all supported Asset objects
   */
  getAssetList(): Asset[] {
    return Object.values(ASSETS);
  }

  /**
   * Validate if a code is a supported asset code.
   * @param code - The code to validate
   * @returns true if the code is a valid asset code (XLM, USDC, or PYUSD), false otherwise
   */
  isValidAssetCode(code: string): boolean {
    return code === 'XLM' || code === 'USDC' || code === 'PYUSD';
  }
}

/**
 * Singleton instance of AssetService for use across the application.
 * Ensures consistent asset metadata and operations throughout the app.
 */
const assetService = new AssetService();

export { AssetService, assetService, ASSETS };
